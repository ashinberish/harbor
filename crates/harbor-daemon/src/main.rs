mod api;
mod desired_state;
mod process_util;
mod supervisor;
mod token;

use std::sync::Arc;

use harbor_core::{GlobalConfig, HarborPaths};
use tracing::info;

use supervisor::Supervisor;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("harbord=info".parse()?))
        .init();

    let paths = HarborPaths::discover();
    paths.ensure_dirs()?;
    info!("harbor home: {}", paths.home.display());

    let global = GlobalConfig::load_or_default(&paths.global_config_file)?;
    global.save(&paths.global_config_file)?;

    let api_token = token::load_or_create(&paths.token_file)?;

    let supervisor = Arc::new(Supervisor::new(paths.clone()));
    supervisor.load_configs()?;

    for name in supervisor.names_with_desired_running() {
        info!("auto-recovering app '{name}'");
        if let Err(e) = supervisor.start_app(&name) {
            tracing::warn!("failed to auto-recover app '{name}': {e}");
        }
    }

    let state = api::AppState {
        supervisor,
        token: Arc::new(api_token),
    };
    let app = api::router(state);

    let listener = tokio::net::TcpListener::bind(&global.bind_addr).await?;
    info!("harbor daemon listening on {}", global.bind_addr);
    axum::serve(listener, app).await?;

    Ok(())
}
