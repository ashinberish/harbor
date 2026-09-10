use harbor_core::{AddAppRequest, AppStatus, ErrorResponse, LogsResponse, ReloadResponse};

pub struct Client {
    http: reqwest::Client,
    base_url: String,
    token: String,
}

impl Client {
    pub fn new(base_url: String, token: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url,
            token,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn check(resp: reqwest::Response) -> anyhow::Result<reqwest::Response> {
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let message = serde_json::from_str::<ErrorResponse>(&body)
            .map(|e| e.error)
            .unwrap_or(body);
        anyhow::bail!("daemon returned {status}: {message}")
    }

    pub async fn list_apps(&self) -> anyhow::Result<Vec<AppStatus>> {
        let resp = self
            .http
            .get(self.url("/apps"))
            .bearer_auth(&self.token)
            .send()
            .await?;
        Ok(Self::check(resp).await?.json().await?)
    }

    pub async fn get_app(&self, name: &str) -> anyhow::Result<AppStatus> {
        let resp = self
            .http
            .get(self.url(&format!("/apps/{name}")))
            .bearer_auth(&self.token)
            .send()
            .await?;
        Ok(Self::check(resp).await?.json().await?)
    }

    pub async fn add_app(&self, req: &AddAppRequest) -> anyhow::Result<AppStatus> {
        let resp = self
            .http
            .post(self.url("/apps"))
            .bearer_auth(&self.token)
            .json(req)
            .send()
            .await?;
        Ok(Self::check(resp).await?.json().await?)
    }

    pub async fn start_app(&self, name: &str) -> anyhow::Result<()> {
        let resp = self
            .http
            .post(self.url(&format!("/apps/{name}/start")))
            .bearer_auth(&self.token)
            .send()
            .await?;
        Self::check(resp).await?;
        Ok(())
    }

    pub async fn stop_app(&self, name: &str) -> anyhow::Result<()> {
        let resp = self
            .http
            .post(self.url(&format!("/apps/{name}/stop")))
            .bearer_auth(&self.token)
            .send()
            .await?;
        Self::check(resp).await?;
        Ok(())
    }

    pub async fn restart_app(&self, name: &str) -> anyhow::Result<()> {
        let resp = self
            .http
            .post(self.url(&format!("/apps/{name}/restart")))
            .bearer_auth(&self.token)
            .send()
            .await?;
        Self::check(resp).await?;
        Ok(())
    }

    pub async fn remove_app(&self, name: &str) -> anyhow::Result<()> {
        let resp = self
            .http
            .delete(self.url(&format!("/apps/{name}")))
            .bearer_auth(&self.token)
            .send()
            .await?;
        Self::check(resp).await?;
        Ok(())
    }

    pub async fn apply(&self) -> anyhow::Result<ReloadResponse> {
        let resp = self
            .http
            .post(self.url("/reload"))
            .bearer_auth(&self.token)
            .send()
            .await?;
        Ok(Self::check(resp).await?.json().await?)
    }

    pub async fn logs(&self, name: &str, lines: usize) -> anyhow::Result<LogsResponse> {
        let resp = self
            .http
            .get(self.url(&format!("/apps/{name}/logs?lines={lines}")))
            .bearer_auth(&self.token)
            .send()
            .await?;
        Ok(Self::check(resp).await?.json().await?)
    }
}
