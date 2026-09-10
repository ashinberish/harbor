mod acme;
mod api;
mod desired_state;
mod process_util;
mod proxy;
mod supervisor;
mod tls;
mod token;
mod watch;
#[cfg(windows)]
mod winservice;

use std::future::Future;
use std::sync::Arc;

use harbor_core::{GlobalConfig, HarborPaths};
use tracing::info;

use supervisor::Supervisor;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("harbord=info".parse()?))
        .init();

    // On Windows, `harbor service install` registers this binary with the
    // Service Control Manager passing `--windows-service`; the SCM starts
    // the process with that flag and expects it to immediately hand
    // control to the service dispatcher rather than run normally. Every
    // other invocation — direct, or under systemd/launchd, which manage
    // arbitrary processes without a comparable startup protocol — just
    // runs the daemon in the foreground.
    #[cfg(windows)]
    if std::env::args().any(|a| a == "--windows-service") {
        return winservice::run();
    }

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(run_daemon(shutdown_signal()))
}

/// Resolves on Ctrl+C, or on SIGTERM on Unix — the signal `systemctl
/// stop`/`launchctl stop` send. Letting the daemon catch this and return
/// from `main()` normally (rather than being killed outright by the
/// default OS disposition) matters because supervised child processes are
/// only guaranteed to be cleaned up via `kill_on_drop` when Rust's normal
/// drop glue runs, which an unhandled signal bypasses.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    info!("shutdown signal received");
}

/// Everything the daemon does, from one invocation to process exit.
/// Shared between normal foreground startup and the Windows service entry
/// point (`winservice.rs`), which supplies SCM stop notifications as
/// `shutdown` instead of OS signals.
pub async fn run_daemon(shutdown: impl Future<Output = ()> + Send + 'static) -> anyhow::Result<()> {
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

    if global.proxy.enabled {
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .expect("no other rustls CryptoProvider installed yet");

        let acme_challenges: proxy::AcmeChallengeStore = Default::default();
        let cert_store = Arc::new(tls::CertStore::new());
        for domain in supervisor.configured_domains() {
            if let Err(e) = cert_store.ensure_self_signed(&domain, &paths.domain_cert_dir(&domain)) {
                tracing::warn!("failed to provision a certificate for '{domain}': {e}");
            }
        }
        if let Err(e) = cert_store.ensure_default_self_signed(&paths.domain_cert_dir("_default")) {
            tracing::warn!("failed to provision the default HTTPS certificate: {e}");
        }

        let https_listener = tokio::net::TcpListener::bind(&global.proxy.https_bind_addr).await;
        let https_port = match &https_listener {
            Ok(listener) => listener.local_addr().ok().map(|a| a.port()),
            Err(_) => None,
        };
        match https_listener {
            Ok(listener) => {
                info!("reverse proxy: HTTPS listening on {}", global.proxy.https_bind_addr);
                let tls_config = tls::build_server_config(cert_store.clone())?;
                let https_state = proxy::ProxyState {
                    supervisor: supervisor.clone(),
                    is_https: true,
                    https_port,
                    https_redirect: global.proxy.https_redirect,
                    acme_challenges: acme_challenges.clone(),
                };
                tokio::spawn(async move {
                    tls::serve_https(listener, tls_config, https_state).await;
                });
            }
            Err(e) => tracing::warn!(
                "reverse proxy: failed to bind HTTPS listener on {}: {e}",
                global.proxy.https_bind_addr
            ),
        }

        let http_state = proxy::ProxyState {
            supervisor: supervisor.clone(),
            is_https: false,
            https_port,
            https_redirect: global.proxy.https_redirect,
            acme_challenges: acme_challenges.clone(),
        };
        let http_bind = global.proxy.http_bind_addr.clone();
        tokio::spawn(async move {
            if let Err(e) = proxy::serve_http(http_bind, http_state).await {
                tracing::error!("reverse proxy HTTP listener failed: {e:#}");
            }
        });

        if https_port.is_some() && global.proxy.acme.enabled {
            let acme_config = global.proxy.acme.clone();
            let paths = paths.clone();
            let supervisor = supervisor.clone();
            tokio::spawn(async move {
                loop {
                    let domains = supervisor.configured_domains();
                    acme::issue_for_domains(
                        &acme_config,
                        &domains,
                        &paths,
                        cert_store.clone(),
                        acme_challenges.clone(),
                    )
                    .await;
                    tokio::time::sleep(std::time::Duration::from_secs(24 * 60 * 60)).await;
                }
            });
        }
    }

    // Kept alive for the daemon's lifetime — dropping it stops the watch.
    let _config_watcher = watch::start(&paths.apps_dir, supervisor.clone())?;

    let state = api::AppState {
        supervisor,
        token: Arc::new(api_token),
    };
    let app = api::router(state);

    let listener = tokio::net::TcpListener::bind(&global.bind_addr).await?;
    info!("harbor daemon listening on {}", global.bind_addr);
    axum::serve(listener, app).with_graceful_shutdown(shutdown).await?;
    info!("harbor daemon stopped");

    Ok(())
}
