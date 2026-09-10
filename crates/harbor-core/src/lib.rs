pub mod api;
pub mod app;
pub mod detect;
pub mod paths;

pub use api::{AddAppRequest, ErrorResponse, LogsResponse, ReloadResponse};
pub use app::{AppConfig, AppState, AppStatus, RestartPolicy, RuntimeKind};
pub use paths::{AcmeConfig, GlobalConfig, HarborPaths, ProxyConfig};

/// Service/unit name used when registering `harbord` with the OS service
/// manager (`harbor service install`, FR7) — shared so the CLI's
/// registration code and the daemon's own Windows service entry point
/// never drift apart.
pub const SERVICE_NAME: &str = "harbord";
