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
    pub cert_dir: PathBuf,
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
            cert_dir: home.join("certs"),
            global_config_file: home.join("harbor.toml"),
            token_file: home.join("token"),
            home,
        }
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for dir in [
            &self.home,
            &self.apps_dir,
            &self.state_dir,
            &self.log_dir,
            &self.cert_dir,
        ] {
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

    /// Directory holding `<domain>/{cert.pem,key.pem,meta.json}` for one
    /// domain's TLS certificate, whether self-signed or ACME-issued.
    pub fn domain_cert_dir(&self, domain: &str) -> PathBuf {
        self.cert_dir.join(domain)
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
    #[serde(default)]
    pub proxy: ProxyConfig,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_bind_addr(),
            proxy: ProxyConfig::default(),
        }
    }
}

fn default_http_bind_addr() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_https_bind_addr() -> String {
    "0.0.0.0:8443".to_string()
}

fn default_true() -> bool {
    true
}

/// Reverse proxy settings (FR8–FR12). Defaults to non-privileged ports so
/// `harbord` runs without elevated privileges out of the box; production
/// deployments fronting real traffic on 80/443 should set these explicitly
/// and grant the daemon permission to bind them (e.g. `setcap
/// cap_net_bind_service`, a systemd `AmbientCapabilities=`, or running as
/// an account with that right on Windows).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// Whether to start the reverse proxy listeners at all. Process
    /// supervision and the management API work independently of this.
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_http_bind_addr")]
    pub http_bind_addr: String,
    #[serde(default = "default_https_bind_addr")]
    pub https_bind_addr: String,
    /// Redirect plain HTTP to HTTPS by default (FR10), except for
    /// ACME HTTP-01 challenge requests, which are always answered directly.
    #[serde(default = "default_true")]
    pub https_redirect: bool,
    #[serde(default)]
    pub acme: AcmeConfig,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            http_bind_addr: default_http_bind_addr(),
            https_bind_addr: default_https_bind_addr(),
            https_redirect: true,
            acme: AcmeConfig::default(),
        }
    }
}

fn default_acme_directory() -> String {
    // Let's Encrypt *staging* by default: safer to opt into production
    // issuance explicitly than to risk hitting production rate limits
    // during testing.
    "https://acme-staging-v02.api.letsencrypt.org/directory".to_string()
}

/// Automatic SSL certificate provisioning via ACME (FR9). Disabled by
/// default — an app with a `domain` set still gets a locally-generated
/// self-signed certificate so HTTPS works out of the box; ACME is opt-in
/// once you have a real, publicly-resolvable domain pointed at this host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcmeConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Contact email registered with the ACME account. Required by most
    /// ACME servers (including Let's Encrypt) when `enabled` is true.
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default = "default_acme_directory")]
    pub directory_url: String,
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            email: None,
            directory_url: default_acme_directory(),
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
