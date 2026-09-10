mod daemon;

use daemon::DaemonClient;
use harbor_core::{AddAppRequest, AppConfig, AppStatus, LogsResponse, ReloadResponse, RuntimeKind};

#[tauri::command]
async fn list_apps() -> Result<Vec<AppStatus>, String> {
    DaemonClient::discover()?.list_apps().await
}

#[tauri::command]
async fn get_app(name: String) -> Result<AppStatus, String> {
    DaemonClient::discover()?.get_app(&name).await
}

#[tauri::command]
async fn add_app(req: AddAppRequest) -> Result<AppStatus, String> {
    DaemonClient::discover()?.add_app(&req).await
}

#[tauri::command]
async fn start_app(name: String) -> Result<(), String> {
    DaemonClient::discover()?.start_app(&name).await
}

#[tauri::command]
async fn stop_app(name: String) -> Result<(), String> {
    DaemonClient::discover()?.stop_app(&name).await
}

#[tauri::command]
async fn restart_app(name: String) -> Result<(), String> {
    DaemonClient::discover()?.restart_app(&name).await
}

#[tauri::command]
async fn remove_app(name: String) -> Result<(), String> {
    DaemonClient::discover()?.remove_app(&name).await
}

#[tauri::command]
async fn apply() -> Result<ReloadResponse, String> {
    DaemonClient::discover()?.apply().await
}

#[tauri::command]
async fn get_logs(name: String, lines: usize) -> Result<LogsResponse, String> {
    DaemonClient::discover()?.logs(&name, lines).await
}

#[tauri::command]
async fn get_app_config(name: String) -> Result<AppConfig, String> {
    DaemonClient::discover()?.get_config(&name).await
}

#[tauri::command]
async fn update_app_config(config: AppConfig) -> Result<AppStatus, String> {
    DaemonClient::discover()?.update_config(&config).await
}

/// Best-effort runtime detection for the add-app wizard (FR1/FR22), run
/// locally against the filesystem rather than round-tripping through the
/// daemon (the daemon only detects at `POST /apps` time).
#[tauri::command]
fn detect_runtime(path: String) -> Option<RuntimeKind> {
    harbor_core::detect::detect_runtime(std::path::Path::new(&path))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            list_apps,
            get_app,
            add_app,
            start_app,
            stop_app,
            restart_app,
            remove_app,
            apply,
            get_logs,
            get_app_config,
            update_app_config,
            detect_runtime,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
