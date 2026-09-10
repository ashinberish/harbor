use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Filesystem layout for a Harbor installation. Config lives under
/// `apps_dir` (plain TOML, meant to be version-controlled per FR15);
/// generated/runtime state (logs, PIDs, the API token) lives under
/// `state_dir` and is not meant to be checked in.
#[derive(Debug, Clone)]
pub struct HarborPaths {
    pub home: PathBuf,
    pub apps_dir: PathBuf,
    pub state_dir: PathBuf,
    pub log_dir: PathBuf,
    pub global_config_file: PathBuf,
    pub token_file: PathBuf,
}

impl HarborPaths {
    /// Resolve paths from `$HARBOR_HOME`, falling back to `~/.harbor`.
    pub fn discover() -> Self {
        let home = std::env::var_os("HARBOR_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".harbor")
            });
        Self::at(home)
    }

    pub fn at(home: PathBuf) -> Self {
        Self {
            apps_dir: home.join("apps"),
            state_dir: home.join("state"),
            log_dir: home.join("logs"),
            global_config_file: home.join("harbor.toml"),
            token_file: home.join("token"),
            home,
        }
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for dir in [&self.home, &self.apps_dir, &self.state_dir, &self.log_dir] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    pub fn app_config_path(&self, name: &str) -> PathBuf {
        self.apps_dir.join(format!("{name}.toml"))
    }

    pub fn app_log_path(&self, name: &str, stream: &str) -> PathBuf {
        self.log_dir.join(format!("{name}.{stream}.log"))
    }
}

fn default_bind_addr() -> String {
    "127.0.0.1:4780".to_string()
}

/// Daemon-wide settings (FR13). Management API is localhost-only by default
/// (FR25) — binding elsewhere requires deliberately editing this file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_bind_addr(),
        }
    }
}

impl GlobalConfig {
    pub fn load_or_default(path: &std::path::Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
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
