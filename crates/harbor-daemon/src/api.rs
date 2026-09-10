use std::sync::Arc;

use axum::extract::{Path as AxPath, Query, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use harbor_core::{AddAppRequest, AppConfig, ErrorResponse, LogsResponse, ReloadResponse, RestartPolicy};
use serde::Deserialize;

use crate::supervisor::Supervisor;

#[derive(Clone)]
pub struct AppState {
    pub supervisor: Arc<Supervisor>,
    pub token: Arc<String>,
}

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/apps", get(list_apps).post(add_app))
        .route(
            "/apps/:name",
            get(get_app).delete(remove_app),
        )
        .route("/apps/:name/start", post(start_app))
        .route("/apps/:name/stop", post(stop_app))
        .route("/apps/:name/restart", post(restart_app))
        .route("/apps/:name/logs", get(get_logs))
        .route("/apps/:name/config", get(get_app_config).put(update_app_config))
        .route("/reload", post(reload))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth));

    Router::new()
        .route("/health", get(health))
        .merge(protected)
        .with_state(state)
}

async fn auth(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|token| token == state.token.as_str())
        .unwrap_or(false);
    if !ok {
        return err(StatusCode::UNAUTHORIZED, "missing or invalid bearer token");
    }
    next.run(req).await
}

async fn health() -> &'static str {
    "ok"
}

async fn list_apps(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.supervisor.status_all())
}

async fn get_app(State(state): State<AppState>, AxPath(name): AxPath<String>) -> Response {
    match state.supervisor.status(&name) {
        Some(status) => Json(status).into_response(),
        None => err(StatusCode::NOT_FOUND, &format!("app '{name}' not found")),
    }
}

async fn add_app(State(state): State<AppState>, Json(req): Json<AddAppRequest>) -> Response {
    let runtime = match req.runtime.or_else(|| harbor_core::detect::detect_runtime(&req.path)) {
        Some(r) => r,
        None => {
            return err(
                StatusCode::UNPROCESSABLE_ENTITY,
                "could not auto-detect runtime; pass --runtime explicitly",
            )
        }
    };
    let name = req.name.unwrap_or_else(|| {
        req.path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "app".to_string())
    });
    let command = req
        .command
        .unwrap_or_else(|| harbor_core::detect::default_command(runtime, &req.path));

    let config = AppConfig {
        name: name.clone(),
        path: req.path,
        runtime,
        command,
        working_dir: None,
        port: req.port,
        domain: req.domain,
        path_prefix: req.path_prefix,
        env: req.env,
        restart_policy: req.restart_policy.unwrap_or(RestartPolicy::OnFailure),
        restart_backoff_seconds: 1,
        restart_max_backoff_seconds: 30,
    };

    match state.supervisor.add_app(config) {
        Ok(()) => match state.supervisor.status(&name) {
            Some(status) => (StatusCode::CREATED, Json(status)).into_response(),
            None => StatusCode::CREATED.into_response(),
        },
        Err(e) => err(StatusCode::CONFLICT, &e.to_string()),
    }
}

async fn remove_app(State(state): State<AppState>, AxPath(name): AxPath<String>) -> Response {
    match state.supervisor.remove_app(&name) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => err(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

async fn start_app(State(state): State<AppState>, AxPath(name): AxPath<String>) -> Response {
    match state.supervisor.start_app(&name) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => err(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

async fn stop_app(State(state): State<AppState>, AxPath(name): AxPath<String>) -> Response {
    match state.supervisor.stop_app(&name) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => err(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

async fn restart_app(State(state): State<AppState>, AxPath(name): AxPath<String>) -> Response {
    match state.supervisor.restart_app(&name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => err(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

#[derive(Deserialize)]
struct LogsQuery {
    lines: Option<usize>,
}

async fn get_logs(
    State(state): State<AppState>,
    AxPath(name): AxPath<String>,
    Query(q): Query<LogsQuery>,
) -> Response {
    let lines = q.lines.unwrap_or(100);
    match state.supervisor.tail_logs(&name, lines) {
        Ok((stdout, stderr)) => Json(LogsResponse { name, stdout, stderr }).into_response(),
        Err(e) => err(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

/// `GET /apps/:name/config` — the app's full declarative config, for a
/// config editor to load and let the user modify (FR24).
async fn get_app_config(State(state): State<AppState>, AxPath(name): AxPath<String>) -> Response {
    match state.supervisor.get_config(&name) {
        Some(cfg) => Json(cfg).into_response(),
        None => err(StatusCode::NOT_FOUND, &format!("app '{name}' not found")),
    }
}

/// `PUT /apps/:name/config` — validate and persist an edited config
/// (FR24). Takes effect for routing immediately; a running process picks
/// up a changed command/env/working_dir on its next start or restart.
async fn update_app_config(
    State(state): State<AppState>,
    AxPath(name): AxPath<String>,
    Json(config): Json<AppConfig>,
) -> Response {
    if config.name != name {
        return err(
            StatusCode::BAD_REQUEST,
            "config 'name' must match the URL and cannot be changed here",
        );
    }
    match state.supervisor.update_config(config) {
        Ok(()) => match state.supervisor.status(&name) {
            Some(status) => Json(status).into_response(),
            None => StatusCode::NO_CONTENT.into_response(),
        },
        Err(e) => err(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string()),
    }
}

/// `POST /reload` — explicit config apply, for when file-watch hot-reload
/// isn't running or a caller wants a synchronous confirmation (FR14).
async fn reload(State(state): State<AppState>) -> Response {
    match state.supervisor.reload_configs() {
        Ok(apps_loaded) => Json(ReloadResponse { apps_loaded }).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

fn err(status: StatusCode, message: &str) -> Response {
    (status, Json(ErrorResponse { error: message.to_string() })).into_response()
}
