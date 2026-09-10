//! Thin HTTP client wrapping the harbord management API, used by the Tauri
//! commands in `lib.rs`. Kept out of the webview: the frontend never sees
//! the bearer token or talks to the daemon directly, it only calls
//! `invoke()` and gets JSON back.

use harbor_core::{
    AddAppRequest, AppConfig, AppStatus, ErrorResponse, GlobalConfig, HarborPaths, LogsResponse,
    ReloadResponse,
};

pub struct DaemonClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
}

impl DaemonClient {
    /// Re-resolves `$HARBOR_HOME`/`~/.harbor`, the bind address, and the
    /// token file on every call. These rarely change, but re-reading them
    /// is cheap and keeps the GUI working across a daemon restart without
    /// requiring the user to relaunch it.
    pub fn discover() -> Result<Self, String> {
        let paths = HarborPaths::discover();
        let global = GlobalConfig::load_or_default(&paths.global_config_file)
            .map_err(|e| format!("reading harbor.toml: {e}"))?;
        let token = std::fs::read_to_string(&paths.token_file)
            .map(|s| s.trim().to_string())
            .map_err(|_| {
                format!(
                    "no daemon token found at {}; is the harbor daemon running?",
                    paths.token_file.display()
                )
            })?;
        Ok(Self {
            http: reqwest::Client::new(),
            base_url: format!("http://{}", global.bind_addr),
            token,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn check(resp: reqwest::Response) -> Result<reqwest::Response, String> {
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let message = serde_json::from_str::<ErrorResponse>(&body)
            .map(|e| e.error)
            .unwrap_or(body);
        Err(format!("daemon returned {status}: {message}"))
    }

    pub async fn list_apps(&self) -> Result<Vec<AppStatus>, String> {
        let resp = self
            .http
            .get(self.url("/apps"))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Self::check(resp).await?.json().await.map_err(|e| e.to_string())
    }

    pub async fn get_app(&self, name: &str) -> Result<AppStatus, String> {
        let resp = self
            .http
            .get(self.url(&format!("/apps/{name}")))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Self::check(resp).await?.json().await.map_err(|e| e.to_string())
    }

    pub async fn add_app(&self, req: &AddAppRequest) -> Result<AppStatus, String> {
        let resp = self
            .http
            .post(self.url("/apps"))
            .bearer_auth(&self.token)
            .json(req)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Self::check(resp).await?.json().await.map_err(|e| e.to_string())
    }

    pub async fn start_app(&self, name: &str) -> Result<(), String> {
        let resp = self
            .http
            .post(self.url(&format!("/apps/{name}/start")))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Self::check(resp).await.map(|_| ())
    }

    pub async fn stop_app(&self, name: &str) -> Result<(), String> {
        let resp = self
            .http
            .post(self.url(&format!("/apps/{name}/stop")))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Self::check(resp).await.map(|_| ())
    }

    pub async fn restart_app(&self, name: &str) -> Result<(), String> {
        let resp = self
            .http
            .post(self.url(&format!("/apps/{name}/restart")))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Self::check(resp).await.map(|_| ())
    }

    pub async fn remove_app(&self, name: &str) -> Result<(), String> {
        let resp = self
            .http
            .delete(self.url(&format!("/apps/{name}")))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Self::check(resp).await.map(|_| ())
    }

    pub async fn apply(&self) -> Result<ReloadResponse, String> {
        let resp = self
            .http
            .post(self.url("/reload"))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Self::check(resp).await?.json().await.map_err(|e| e.to_string())
    }

    pub async fn logs(&self, name: &str, lines: usize) -> Result<LogsResponse, String> {
        let resp = self
            .http
            .get(self.url(&format!("/apps/{name}/logs?lines={lines}")))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Self::check(resp).await?.json().await.map_err(|e| e.to_string())
    }

    pub async fn get_config(&self, name: &str) -> Result<AppConfig, String> {
        let resp = self
            .http
            .get(self.url(&format!("/apps/{name}/config")))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Self::check(resp).await?.json().await.map_err(|e| e.to_string())
    }

    pub async fn update_config(&self, config: &AppConfig) -> Result<AppStatus, String> {
        let resp = self
            .http
            .put(self.url(&format!("/apps/{}/config", config.name)))
            .bearer_auth(&self.token)
            .json(config)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        Self::check(resp).await?.json().await.map_err(|e| e.to_string())
    }
}
