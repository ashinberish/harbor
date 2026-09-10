//! Windows Service Control Manager (SCM) integration (FR7).
//!
//! Unlike systemd/launchd, which supervise an arbitrary process without it
//! needing to know that, Windows expects a service's process to actively
//! participate in the SCM protocol: register a control handler, report
//! `Running` shortly after starting, and report `Stopped` (in response to
//! a `Stop` control) before exiting — a plain console app registered with
//! `sc.exe create` without doing this gets killed by the SCM rather than
//! stopped cleanly. `harbor service install` (see `harbor-cli`) passes
//! `--windows-service` so `main()` routes here instead of running the
//! daemon directly.

use std::ffi::OsString;
use std::sync::mpsc;
use std::time::Duration;

use harbor_core::SERVICE_NAME;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::{define_windows_service, service_dispatcher};

const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

/// Entry point called from `main()`. Hands control to the SCM dispatcher,
/// which blocks this thread until Windows starts the service and calls
/// back into `service_main` below — it does not return until the service
/// stops.
pub fn run() -> anyhow::Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        .map_err(|e| anyhow::anyhow!("starting Windows service dispatcher: {e}"))
}

define_windows_service!(ffi_service_main, service_main);

fn service_main(_arguments: Vec<OsString>) {
    if let Err(e) = run_service() {
        tracing::error!("Windows service run failed: {e:#}");
    }
}

fn run_service() -> anyhow::Result<()> {
    // Bridges the SCM's synchronous, off-runtime stop notification into
    // the async `shutdown` future `run_daemon` expects.
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            ServiceControl::Stop => {
                let _ = shutdown_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)
        .map_err(|e| anyhow::anyhow!("registering service control handler: {e}"))?;

    status_handle
        .set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Running,
            controls_accepted: ServiceControlAccept::STOP,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
        .map_err(|e| anyhow::anyhow!("reporting Running status to SCM: {e}"))?;

    let rt = tokio::runtime::Runtime::new()?;
    let shutdown = async move {
        let _ = tokio::task::spawn_blocking(move || shutdown_rx.recv()).await;
    };
    let result = rt.block_on(crate::run_daemon(shutdown));

    let exit_code = match &result {
        Ok(()) => ServiceExitCode::Win32(0),
        Err(_) => ServiceExitCode::ServiceSpecific(1),
    };
    status_handle
        .set_service_status(ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code,
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        })
        .map_err(|e| anyhow::anyhow!("reporting Stopped status to SCM: {e}"))?;

    result
}
