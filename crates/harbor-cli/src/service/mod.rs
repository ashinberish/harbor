//! `harbor service` — install/manage `harbord` as a native OS service
//! (FR7), so it starts at boot and is supervised by the OS the same way a
//! real deployment expects, instead of needing a terminal left open (or a
//! separate wrapper like NSSM on Windows).

#[cfg(target_os = "linux")]
mod systemd;

#[cfg(target_os = "macos")]
mod launchd;

#[cfg(windows)]
mod windows;

use std::path::PathBuf;

/// Find the `harbord` binary to register, defaulting to a binary named
/// `harbord`(`.exe`) next to this `harbor` executable — true for a cargo
/// build (`target/debug|release/`) and for a typical installed layout
/// (both binaries in the same `bin/` directory).
fn locate_binary(override_path: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    if let Some(path) = override_path {
        if !path.is_file() {
            anyhow::bail!("--binary {} does not exist", path.display());
        }
        return Ok(path);
    }

    let exe = std::env::current_exe().map_err(|e| anyhow::anyhow!("locating the current executable: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("could not determine the directory of {}", exe.display()))?;
    let name = if cfg!(windows) { "harbord.exe" } else { "harbord" };
    let candidate = dir.join(name);
    if candidate.is_file() {
        Ok(candidate)
    } else {
        anyhow::bail!(
            "could not find '{name}' next to this CLI binary at {}; pass --binary <path>",
            candidate.display()
        )
    }
}

pub fn install(user: bool, binary: Option<PathBuf>) -> anyhow::Result<()> {
    let binary = locate_binary(binary)?;

    #[cfg(target_os = "linux")]
    return systemd::install(user, &binary);
    #[cfg(target_os = "macos")]
    return launchd::install(user, &binary);
    #[cfg(windows)]
    {
        let _ = user; // Windows services are inherently system-level.
        windows::install(&binary)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = binary;
        anyhow::bail!("`harbor service` isn't supported on this platform");
    }
}

pub fn uninstall(user: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    return systemd::uninstall(user);
    #[cfg(target_os = "macos")]
    return launchd::uninstall(user);
    #[cfg(windows)]
    {
        let _ = user;
        windows::uninstall()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    anyhow::bail!("`harbor service` isn't supported on this platform");
}

pub fn start(user: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    return systemd::start(user);
    #[cfg(target_os = "macos")]
    return launchd::start(user);
    #[cfg(windows)]
    {
        let _ = user;
        windows::start()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    anyhow::bail!("`harbor service` isn't supported on this platform");
}

pub fn stop(user: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    return systemd::stop(user);
    #[cfg(target_os = "macos")]
    return launchd::stop(user);
    #[cfg(windows)]
    {
        let _ = user;
        windows::stop()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    anyhow::bail!("`harbor service` isn't supported on this platform");
}

pub fn status(user: bool) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    return systemd::status(user);
    #[cfg(target_os = "macos")]
    return launchd::status(user);
    #[cfg(windows)]
    {
        let _ = user;
        windows::status()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    anyhow::bail!("`harbor service` isn't supported on this platform");
}
