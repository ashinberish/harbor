use std::ffi::OsString;
use std::path::Path;
use std::time::{Duration, Instant};

use harbor_core::SERVICE_NAME;
use windows_service::service::{
    ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceState, ServiceType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_sys::Win32::Foundation::ERROR_SERVICE_DOES_NOT_EXIST;

fn connect(access: ServiceManagerAccess) -> anyhow::Result<ServiceManager> {
    ServiceManager::local_computer(None::<&str>, access)
        .map_err(|e| anyhow::anyhow!("connecting to the Service Control Manager: {e}"))
}

pub fn install(binary: &Path) -> anyhow::Result<()> {
    let manager = connect(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;

    let service_info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from("Harbor daemon"),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: binary.to_path_buf(),
        // Tells harbord's main() to hand control to the SCM dispatcher
        // (winservice.rs) instead of running in the foreground.
        launch_arguments: vec![OsString::from("--windows-service")],
        dependencies: vec![],
        account_name: None, // Local System
        account_password: None,
    };
    let service = manager
        .create_service(&service_info, ServiceAccess::CHANGE_CONFIG | ServiceAccess::START)
        .map_err(|e| anyhow::anyhow!("creating service (try running as Administrator): {e}"))?;
    service
        .set_description("Harbor daemon — process supervision and reverse proxy")
        .map_err(|e| anyhow::anyhow!("setting service description: {e}"))?;
    service
        .start::<&str>(&[])
        .map_err(|e| anyhow::anyhow!("starting service: {e}"))?;

    println!("installed and started {SERVICE_NAME}");
    Ok(())
}

pub fn uninstall() -> anyhow::Result<()> {
    let manager = connect(ServiceManagerAccess::CONNECT)?;
    let service_access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
    let service = manager
        .open_service(SERVICE_NAME, service_access)
        .map_err(|e| anyhow::anyhow!("opening service (try running as Administrator): {e}"))?;

    // Marks it for deletion; it's actually removed once stopped and every
    // open handle to it (including `service` below) is closed.
    service.delete().map_err(|e| anyhow::anyhow!("deleting service: {e}"))?;
    if service
        .query_status()
        .map_err(|e| anyhow::anyhow!("querying service status: {e}"))?
        .current_state
        != ServiceState::Stopped
    {
        let _ = service.stop();
    }
    drop(service);

    let start = Instant::now();
    let timeout = Duration::from_secs(5);
    while start.elapsed() < timeout {
        match manager.open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS) {
            Err(windows_service::Error::Winapi(e))
                if e.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST as i32) =>
            {
                println!("uninstalled {SERVICE_NAME}");
                return Ok(());
            }
            _ => std::thread::sleep(Duration::from_millis(200)),
        }
    }
    println!("{SERVICE_NAME} is marked for deletion and will be removed once fully stopped");
    Ok(())
}

pub fn start() -> anyhow::Result<()> {
    let manager = connect(ServiceManagerAccess::CONNECT)?;
    let service = manager
        .open_service(SERVICE_NAME, ServiceAccess::START)
        .map_err(|e| anyhow::anyhow!("opening service: {e}"))?;
    service
        .start::<&str>(&[])
        .map_err(|e| anyhow::anyhow!("starting service: {e}"))?;
    println!("started {SERVICE_NAME}");
    Ok(())
}

pub fn stop() -> anyhow::Result<()> {
    let manager = connect(ServiceManagerAccess::CONNECT)?;
    let service = manager
        .open_service(SERVICE_NAME, ServiceAccess::STOP)
        .map_err(|e| anyhow::anyhow!("opening service: {e}"))?;
    service.stop().map_err(|e| anyhow::anyhow!("stopping service: {e}"))?;
    println!("stopped {SERVICE_NAME}");
    Ok(())
}

pub fn status() -> anyhow::Result<()> {
    let manager = connect(ServiceManagerAccess::CONNECT)?;
    let service = manager
        .open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS)
        .map_err(|e| anyhow::anyhow!("opening service: {e}"))?;
    let status = service
        .query_status()
        .map_err(|e| anyhow::anyhow!("querying service status: {e}"))?;
    println!("{SERVICE_NAME}: {:?}", status.current_state);
    Ok(())
}
