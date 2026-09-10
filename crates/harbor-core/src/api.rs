use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::app::{RestartPolicy, RuntimeKind};

/// Body of `POST /apps` (FR16: `harbor add <path> [--runtime] [--port] [--domain]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddAppRequest {
    pub path: PathBuf,
    pub name: Option<String>,
    pub runtime: Option<RuntimeKind>,
    pub command: Option<Vec<String>>,
    pub port: Option<u16>,
    pub domain: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub restart_policy: Option<RestartPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsResponse {
    pub name: String,
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}
