use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};
use harbor_core::{AppConfig, AppState, AppStatus, HarborPaths, RestartPolicy};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::desired_state::{self, DesiredState};

enum ControlMsg {
    Stop,
}

struct RuntimeInfo {
    state: AppState,
    pid: Option<u32>,
    restart_count: u32,
    started_at: Option<DateTime<Utc>>,
    last_exit_code: Option<i32>,
    control_tx: Option<mpsc::UnboundedSender<ControlMsg>>,
}

impl Default for RuntimeInfo {
    fn default() -> Self {
        Self {
            state: AppState::Stopped,
            pid: None,
            restart_count: 0,
            started_at: None,
            last_exit_code: None,
            control_tx: None,
        }
    }
}

pub struct Supervisor {
    pub paths: HarborPaths,
    configs: Mutex<HashMap<String, AppConfig>>,
    runtime: Mutex<HashMap<String, RuntimeInfo>>,
}

impl Supervisor {
    pub fn new(paths: HarborPaths) -> Self {
        Self {
            paths,
            configs: Mutex::new(HashMap::new()),
            runtime: Mutex::new(HashMap::new()),
        }
    }

    /// Load every `<apps_dir>/*.toml` app config from disk into memory.
    pub fn load_configs(&self) -> anyhow::Result<()> {
        self.reload_configs()?;
        Ok(())
    }

    pub fn names_with_desired_running(&self) -> Vec<String> {
        let configs = self.configs.lock().unwrap();
        configs
            .keys()
            .filter(|name| desired_state::load(&self.paths.state_dir, name).running)
            .cloned()
            .collect()
    }

    pub fn add_app(&self, config: AppConfig) -> anyhow::Result<()> {
        {
            let configs = self.configs.lock().unwrap();
            if configs.contains_key(&config.name) {
                anyhow::bail!("app '{}' already exists", config.name);
            }
        }
        config.save(&self.paths.app_config_path(&config.name))?;
        let mut configs = self.configs.lock().unwrap();
        let mut runtime = self.runtime.lock().unwrap();
        runtime.entry(config.name.clone()).or_default();
        configs.insert(config.name.clone(), config);
        Ok(())
    }

    pub fn remove_app(self: &std::sync::Arc<Self>, name: &str) -> anyhow::Result<()> {
        if !self.exists(name) {
            anyhow::bail!("app '{name}' not found");
        }
        self.stop_app(name)?;
        let _ = std::fs::remove_file(self.paths.app_config_path(name));
        desired_state::remove(&self.paths.state_dir, name);
        self.configs.lock().unwrap().remove(name);
        self.runtime.lock().unwrap().remove(name);
        Ok(())
    }

    pub fn exists(&self, name: &str) -> bool {
        self.configs.lock().unwrap().contains_key(name)
    }

    pub fn start_app(self: &std::sync::Arc<Self>, name: &str) -> anyhow::Result<()> {
        if !self.exists(name) {
            anyhow::bail!("app '{name}' not found");
        }
        let already_running = {
            let runtime = self.runtime.lock().unwrap();
            runtime
                .get(name)
                .map(|r| r.control_tx.is_some())
                .unwrap_or(false)
        };
        // Preserve any previously recorded pid (needed by reap_stale_orphan
        // to find and reap an orphaned process from an earlier daemon
        // instance); the supervised task overwrites it once it spawns.
        let previous_pid = desired_state::load(&self.paths.state_dir, name).pid;
        desired_state::save(
            &self.paths.state_dir,
            name,
            DesiredState {
                running: true,
                pid: previous_pid,
            },
        )?;
        if already_running {
            return Ok(());
        }
        let sup = self.clone();
        let name = name.to_string();
        tokio::spawn(async move { run_app_supervised(sup, name).await });
        Ok(())
    }

    pub fn stop_app(&self, name: &str) -> anyhow::Result<()> {
        if !self.exists(name) {
            anyhow::bail!("app '{name}' not found");
        }
        desired_state::save(&self.paths.state_dir, name, DesiredState { running: false, pid: None })?;
        let tx = {
            let runtime = self.runtime.lock().unwrap();
            runtime.get(name).and_then(|r| r.control_tx.clone())
        };
        if let Some(tx) = tx {
            let _ = tx.send(ControlMsg::Stop);
        }
        Ok(())
    }

    pub async fn restart_app(self: &std::sync::Arc<Self>, name: &str) -> anyhow::Result<()> {
        self.stop_app(name)?;
        for _ in 0..100 {
            let running = {
                let runtime = self.runtime.lock().unwrap();
                runtime
                    .get(name)
                    .map(|r| r.control_tx.is_some())
                    .unwrap_or(false)
            };
            if !running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        self.start_app(name)
    }

    pub fn status(&self, name: &str) -> Option<AppStatus> {
        let configs = self.configs.lock().unwrap();
        let cfg = configs.get(name)?;
        let runtime = self.runtime.lock().unwrap();
        let info = runtime.get(name);
        Some(build_status(name, cfg, info))
    }

    pub fn status_all(&self) -> Vec<AppStatus> {
        let configs = self.configs.lock().unwrap();
        let runtime = self.runtime.lock().unwrap();
        let mut out: Vec<AppStatus> = configs
            .iter()
            .map(|(name, cfg)| build_status(name, cfg, runtime.get(name)))
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn tail_logs(&self, name: &str, lines: usize) -> anyhow::Result<(Vec<String>, Vec<String>)> {
        if !self.exists(name) {
            anyhow::bail!("app '{name}' not found");
        }
        let stdout = tail_file(&self.paths.app_log_path(name, "out"), lines);
        let stderr = tail_file(&self.paths.app_log_path(name, "err"), lines);
        Ok((stdout, stderr))
    }

    /// Re-read every `<apps_dir>/*.toml` from disk, adding newly-created
    /// apps and updating in-memory config for existing ones (FR14). Apps
    /// whose file was deleted out-of-band are left running under their
    /// last-loaded config until explicitly removed — this only picks up
    /// additions and edits, matching `harbor apply`'s job of applying
    /// config changes, not detecting manual file deletion.
    pub fn reload_configs(&self) -> anyhow::Result<usize> {
        if !self.paths.apps_dir.is_dir() {
            return Ok(0);
        }
        let mut loaded = 0;
        let mut configs = self.configs.lock().unwrap();
        let mut runtime = self.runtime.lock().unwrap();
        for entry in std::fs::read_dir(&self.paths.apps_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            match AppConfig::load(&path) {
                Ok(cfg) => {
                    runtime.entry(cfg.name.clone()).or_default();
                    configs.insert(cfg.name.clone(), cfg);
                    loaded += 1;
                }
                Err(e) => warn!("skipping invalid app config {}: {e}", path.display()),
            }
        }
        Ok(loaded)
    }

    /// Find the app whose `domain` or `path_prefix` matches an incoming
    /// proxied request (FR8). An exact `Host` match always wins over a
    /// path-prefix match; among path-prefix matches, the longest prefix
    /// wins. Only apps with a `port` configured are reachable this way.
    pub fn resolve_route(&self, host: Option<&str>, path: &str) -> Option<RouteMatch> {
        let configs = self.configs.lock().unwrap();
        let host_norm = host.map(|h| h.split(':').next().unwrap_or(h).to_ascii_lowercase());

        if let Some(h) = &host_norm {
            for cfg in configs.values() {
                let Some(port) = cfg.port else { continue };
                if cfg.domain.as_deref().is_some_and(|d| d.eq_ignore_ascii_case(h)) {
                    return Some(RouteMatch {
                        app_name: cfg.name.clone(),
                        target_port: port,
                        strip_prefix: None,
                    });
                }
            }
        }

        let mut best: Option<(&AppConfig, u16, usize)> = None;
        for cfg in configs.values() {
            let (Some(prefix), Some(port)) = (&cfg.path_prefix, cfg.port) else {
                continue;
            };
            if path.starts_with(prefix.as_str()) {
                let len = prefix.len();
                if best.map(|(_, _, best_len)| len > best_len).unwrap_or(true) {
                    best = Some((cfg, port, len));
                }
            }
        }
        best.map(|(cfg, port, _)| RouteMatch {
            app_name: cfg.name.clone(),
            target_port: port,
            strip_prefix: cfg.path_prefix.clone(),
        })
    }

    /// Every distinct domain configured across all apps — used to decide
    /// which domains need a TLS certificate (self-signed or ACME).
    pub fn configured_domains(&self) -> Vec<String> {
        let configs = self.configs.lock().unwrap();
        let mut domains: Vec<String> = configs
            .values()
            .filter_map(|cfg| cfg.domain.clone())
            .collect();
        domains.sort();
        domains.dedup();
        domains
    }
}

/// Result of matching an incoming proxy request to a managed app.
#[derive(Debug, Clone)]
pub struct RouteMatch {
    pub app_name: String,
    pub target_port: u16,
    /// Path prefix to strip before forwarding, when matched by path rather
    /// than by domain.
    pub strip_prefix: Option<String>,
}

fn build_status(name: &str, cfg: &AppConfig, info: Option<&RuntimeInfo>) -> AppStatus {
    let (state, pid, restart_count, started_at, last_exit_code) = match info {
        Some(i) => (i.state, i.pid, i.restart_count, i.started_at, i.last_exit_code),
        None => (AppState::Stopped, None, 0, None, None),
    };
    let uptime_seconds = started_at.map(|t| (Utc::now() - t).num_seconds().max(0) as u64);
    AppStatus {
        name: name.to_string(),
        state,
        pid,
        restart_count,
        uptime_seconds,
        runtime: cfg.runtime,
        port: cfg.port,
        domain: cfg.domain.clone(),
        path_prefix: cfg.path_prefix.clone(),
        last_exit_code,
    }
}

fn tail_file(path: &std::path::Path, lines: usize) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].iter().map(|s| s.to_string()).collect()
}

/// If a previous daemon instance recorded a PID for this app and that
/// process is still alive (e.g. the daemon was killed uncleanly and left
/// the app running as an orphan), terminate it first so auto-recovery
/// doesn't end up running two concurrent copies of the same app.
async fn reap_stale_orphan(sup: &std::sync::Arc<Supervisor>, name: &str) {
    let Some(pid) = desired_state::load(&sup.paths.state_dir, name).pid else {
        return;
    };
    if !crate::process_util::is_process_alive(pid) {
        return;
    }
    warn!("app '{name}': found live orphan process (pid={pid}) from a previous daemon instance; terminating it");
    crate::process_util::kill_process(pid);
    for _ in 0..50 {
        if !crate::process_util::is_process_alive(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    warn!("app '{name}': orphan process (pid={pid}) did not exit after SIGTERM");
}

async fn run_app_supervised(sup: std::sync::Arc<Supervisor>, name: String) {
    reap_stale_orphan(&sup, &name).await;

    loop {
        let config = { sup.configs.lock().unwrap().get(&name).cloned() };
        let Some(config) = config else {
            break;
        };
        if config.command.is_empty() {
            warn!("app '{name}' has no launch command configured; not starting");
            let mut runtime = sup.runtime.lock().unwrap();
            if let Some(entry) = runtime.get_mut(&name) {
                entry.state = AppState::Crashed;
            }
            break;
        }

        let stdout_file = match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(sup.paths.app_log_path(&name, "out"))
        {
            Ok(f) => f,
            Err(e) => {
                warn!("app '{name}': failed to open stdout log: {e}");
                break;
            }
        };
        let stderr_file = match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(sup.paths.app_log_path(&name, "err"))
        {
            Ok(f) => f,
            Err(e) => {
                warn!("app '{name}': failed to open stderr log: {e}");
                break;
            }
        };

        let mut cmd = tokio::process::Command::new(&config.command[0]);
        cmd.args(&config.command[1..]);
        cmd.current_dir(config.effective_working_dir());
        cmd.envs(&config.env);
        cmd.stdout(Stdio::from(stdout_file));
        cmd.stderr(Stdio::from(stderr_file));
        cmd.kill_on_drop(true);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                warn!("app '{name}': failed to spawn: {e}");
                let mut runtime = sup.runtime.lock().unwrap();
                if let Some(entry) = runtime.get_mut(&name) {
                    entry.state = AppState::Crashed;
                    entry.control_tx = None;
                }
                break;
            }
        };
        let pid = child.id();
        info!("app '{name}' started (pid={pid:?})");
        if let Some(pid) = pid {
            let _ = desired_state::save(&sup.paths.state_dir, &name, DesiredState { running: true, pid: Some(pid) });
        }

        let (tx, mut rx) = mpsc::unbounded_channel::<ControlMsg>();
        {
            let mut runtime = sup.runtime.lock().unwrap();
            let entry = runtime.entry(name.clone()).or_default();
            entry.state = AppState::Running;
            entry.pid = pid;
            entry.started_at = Some(Utc::now());
            entry.control_tx = Some(tx);
        }

        let stopped_deliberately = tokio::select! {
            status = child.wait() => {
                handle_exit(&sup, &name, &config, status).await;
                false
            }
            _ = rx.recv() => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                true
            }
        };

        if stopped_deliberately {
            let mut runtime = sup.runtime.lock().unwrap();
            if let Some(entry) = runtime.get_mut(&name) {
                entry.state = AppState::Stopped;
                entry.pid = None;
                entry.control_tx = None;
            }
            info!("app '{name}' stopped");
            break;
        }

        let should_restart = {
            let runtime = sup.runtime.lock().unwrap();
            runtime
                .get(&name)
                .map(|r| r.state == AppState::Restarting)
                .unwrap_or(false)
        };
        if !should_restart {
            break;
        }

        let restart_count = {
            let runtime = sup.runtime.lock().unwrap();
            runtime.get(&name).map(|r| r.restart_count).unwrap_or(0)
        };
        let backoff = config
            .restart_backoff_seconds
            .max(1)
            .saturating_mul(1u64 << restart_count.min(10))
            .min(config.restart_max_backoff_seconds.max(config.restart_backoff_seconds.max(1)));
        info!("app '{name}' restarting in {backoff}s (attempt {restart_count})");
        tokio::time::sleep(Duration::from_secs(backoff)).await;

        if !desired_state::load(&sup.paths.state_dir, &name).running {
            let mut runtime = sup.runtime.lock().unwrap();
            if let Some(entry) = runtime.get_mut(&name) {
                entry.state = AppState::Stopped;
            }
            info!("app '{name}' stop requested during backoff; not restarting");
            break;
        }
    }
}

async fn handle_exit(
    sup: &std::sync::Arc<Supervisor>,
    name: &str,
    config: &AppConfig,
    status: std::io::Result<std::process::ExitStatus>,
) {
    let (code, success) = match &status {
        Ok(s) => (s.code(), s.success()),
        Err(_) => (None, false),
    };

    let should_restart = match config.restart_policy {
        RestartPolicy::Never => false,
        RestartPolicy::Always => true,
        RestartPolicy::OnFailure => !success,
    };

    let mut runtime = sup.runtime.lock().unwrap();
    let entry = runtime.entry(name.to_string()).or_default();
    entry.last_exit_code = code;
    entry.pid = None;
    entry.control_tx = None;
    if should_restart {
        entry.state = AppState::Restarting;
        entry.restart_count += 1;
        info!("app '{name}' exited (code={code:?}); will restart per policy");
    } else {
        entry.state = if success { AppState::Stopped } else { AppState::Crashed };
        info!("app '{name}' exited (code={code:?}); not restarting");
    }
}
