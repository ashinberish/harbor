use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use tracing::{info, warn};

use crate::supervisor::Supervisor;

/// Watch `apps_dir` for changes and reload app configs automatically
/// (FR14 hot-reload). The returned watcher must be kept alive for the
/// duration of the daemon — dropping it stops the watch.
pub fn start(apps_dir: &Path, supervisor: Arc<Supervisor>) -> anyhow::Result<notify::RecommendedWatcher> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(16);

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = tx.blocking_send(());
        }
    })?;
    watcher.watch(apps_dir, RecursiveMode::NonRecursive)?;
    info!("watching {} for config changes", apps_dir.display());

    tokio::spawn(async move {
        while rx.recv().await.is_some() {
            // Editors and `cp`/`mv` typically produce a burst of several
            // filesystem events per logical save; wait for the burst to
            // settle before reloading instead of doing it once per event.
            tokio::time::sleep(Duration::from_millis(300)).await;
            while rx.try_recv().is_ok() {}

            match supervisor.reload_configs() {
                Ok(n) => info!("config hot-reload: picked up {n} app config(s)"),
                Err(e) => warn!("config hot-reload failed: {e}"),
            }
        }
    });

    Ok(watcher)
}
