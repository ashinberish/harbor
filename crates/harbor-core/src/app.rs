use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Language runtime detected (or declared) for a managed app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeKind {
    Python,
    Node,
    DotNet,
    Java,
    Rust,
    Custom,
}

impl RuntimeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RuntimeKind::Python => "python",
            RuntimeKind::Node => "node",
            RuntimeKind::DotNet => "dotnet",
            RuntimeKind::Java => "java",
            RuntimeKind::Rust => "rust",
            RuntimeKind::Custom => "custom",
        }
    }
}

impl std::fmt::Display for RuntimeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RuntimeKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "python" | "py" => Ok(RuntimeKind::Python),
            "node" | "nodejs" | "js" => Ok(RuntimeKind::Node),
            "dotnet" | ".net" | "csharp" => Ok(RuntimeKind::DotNet),
            "java" => Ok(RuntimeKind::Java),
            "rust" | "rs" => Ok(RuntimeKind::Rust),
            "custom" => Ok(RuntimeKind::Custom),
            other => anyhow::bail!("unknown runtime '{other}'"),
        }
    }
}

/// Restart behavior applied when a managed app's process exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum RestartPolicy {
    Never,
    #[default]
    OnFailure,
    Always,
}

impl FromStr for RestartPolicy {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "never" => Ok(RestartPolicy::Never),
            "on-failure" | "onfailure" | "on_failure" => Ok(RestartPolicy::OnFailure),
            "always" => Ok(RestartPolicy::Always),
            other => anyhow::bail!("unknown restart policy '{other}'"),
        }
    }
}

fn default_backoff_seconds() -> u64 {
    1
}

fn default_max_backoff_seconds() -> u64 {
    30
}

/// Declarative, version-controllable configuration for a single managed app.
/// Persisted as `<apps_dir>/<name>.toml` (FR13).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub name: String,
    pub path: PathBuf,
    pub runtime: RuntimeKind,
    /// Launch command, e.g. `["python3", "app.py"]`. First element is the
    /// executable, remaining elements are arguments (FR2 manual override).
    pub command: Vec<String>,
    #[serde(default)]
    pub working_dir: Option<PathBuf>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub domain: Option<String>,
    /// URL path prefix routed to this app, e.g. `/api` (FR8). Stripped
    /// before the request is forwarded. Independent of `domain` — either,
    /// both, or neither may be set; an app with neither is never reachable
    /// through the reverse proxy.
    #[serde(default)]
    pub path_prefix: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub restart_policy: RestartPolicy,
    #[serde(default = "default_backoff_seconds")]
    pub restart_backoff_seconds: u64,
    #[serde(default = "default_max_backoff_seconds")]
    pub restart_max_backoff_seconds: u64,
}

impl AppConfig {
    pub fn effective_working_dir(&self) -> PathBuf {
        self.working_dir.clone().unwrap_or_else(|| self.path.clone())
    }

    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
        let cfg: AppConfig = toml::from_str(&text)
            .map_err(|e| anyhow::anyhow!("parsing {}: {e}", path.display()))?;
        Ok(cfg)
    }

    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let text = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)?;
        Ok(())
    }
}

/// Runtime status of a managed app, as reported by the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppState {
    Stopped,
    Running,
    Restarting,
    Crashed,
}

impl std::fmt::Display for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            AppState::Stopped => "stopped",
            AppState::Running => "running",
            AppState::Restarting => "restarting",
            AppState::Crashed => "crashed",
        };
        f.write_str(s)
    }
}

/// Point-in-time status summary for one app, returned by `GET /apps` and
/// `GET /apps/:name` (FR3, FR21).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStatus {
    pub name: String,
    pub state: AppState,
    pub pid: Option<u32>,
    pub restart_count: u32,
    pub uptime_seconds: Option<u64>,
    /// CPU usage as a percentage (0-100 per core, so may exceed 100 on a
    /// multi-core system) sampled roughly every 2 seconds. `None` when the
    /// app isn't running or a sample isn't available yet (FR21).
    pub cpu_percent: Option<f32>,
    /// Resident memory in bytes, same sampling cadence as `cpu_percent`
    /// (FR21).
    pub memory_bytes: Option<u64>,
    pub runtime: RuntimeKind,
    pub port: Option<u16>,
    pub domain: Option<String>,
    pub path_prefix: Option<String>,
    pub last_exit_code: Option<i32>,
}
