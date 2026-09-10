use std::path::Path;

use serde::{Deserialize, Serialize};

/// Whether an app should be running, persisted outside the (version
/// controlled) app config so daemon restarts can auto-recover previously
/// running apps (NFR3, G6) without that runtime detail leaking into the
/// declarative TOML config.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct DesiredState {
    pub running: bool,
    /// PID of the last process spawned for this app, so a fresh daemon
    /// instance can detect and reap an orphan left behind by an unclean
    /// shutdown of a previous daemon instance, instead of spawning a
    /// second concurrent copy of the app during auto-recovery.
    #[serde(default)]
    pub pid: Option<u32>,
}

pub fn load(state_dir: &Path, name: &str) -> DesiredState {
    let path = state_dir.join(format!("{name}.state.json"));
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(state_dir: &Path, name: &str, state: DesiredState) -> anyhow::Result<()> {
    let path = state_dir.join(format!("{name}.state.json"));
    std::fs::write(path, serde_json::to_string(&state)?)?;
    Ok(())
}

pub fn remove(state_dir: &Path, name: &str) {
    let path = state_dir.join(format!("{name}.state.json"));
    let _ = std::fs::remove_file(path);
}
