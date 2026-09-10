use std::path::{Path, PathBuf};
use std::process::Command;

use harbor_core::SERVICE_NAME;

const LABEL: &str = "com.harbor.harbord";

fn plist_path(user: bool) -> anyhow::Result<PathBuf> {
    if user {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
        Ok(home.join("Library/LaunchAgents").join(format!("{LABEL}.plist")))
    } else {
        Ok(PathBuf::from("/Library/LaunchDaemons").join(format!("{LABEL}.plist")))
    }
}

/// `launchctl`'s modern (10.10+) subcommands (`bootstrap`/`bootout`/
/// `kickstart`/`print`) address a job as `<domain-target>/<label>` —
/// `system` for a LaunchDaemon, `gui/<uid>` for a LaunchAgent running in a
/// user's graphical session.
fn domain(user: bool) -> anyhow::Result<String> {
    if !user {
        return Ok("system".to_string());
    }
    let output = Command::new("id")
        .arg("-u")
        .output()
        .map_err(|e| anyhow::anyhow!("running `id -u`: {e}"))?;
    let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if uid.is_empty() {
        anyhow::bail!("could not determine the current user's UID");
    }
    Ok(format!("gui/{uid}"))
}

fn plist_contents(binary: &Path, user: bool) -> anyhow::Result<String> {
    let log_dir: PathBuf = if user {
        dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?
            .join(".harbor/logs")
    } else {
        PathBuf::from("/var/log")
    };
    let stdout_path = log_dir.join("harbord.stdout.log");
    let stderr_path = log_dir.join("harbord.stderr.log");

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{binary}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<true/>
	<key>StandardOutPath</key>
	<string>{stdout_path}</string>
	<key>StandardErrorPath</key>
	<string>{stderr_path}</string>
</dict>
</plist>
"#,
        binary = binary.display(),
        stdout_path = stdout_path.display(),
        stderr_path = stderr_path.display(),
    ))
}

fn launchctl(args: &[&str]) -> anyhow::Result<std::process::ExitStatus> {
    Command::new("launchctl")
        .args(args)
        .status()
        .map_err(|e| anyhow::anyhow!("running launchctl: {e}"))
}

fn launchctl_checked(args: &[&str]) -> anyhow::Result<()> {
    let status = launchctl(args)?;
    if !status.success() {
        anyhow::bail!("launchctl {} exited with {status}", args.join(" "));
    }
    Ok(())
}

pub fn install(user: bool, binary: &Path) -> anyhow::Result<()> {
    let path = plist_path(user)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("creating {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, plist_contents(binary, user)?).map_err(|e| {
        anyhow::anyhow!(
            "writing {}: {e}{}",
            path.display(),
            if user { "" } else { " (a system-wide install needs root — re-run with sudo, or pass --user)" }
        )
    })?;

    let target = format!("{}/{LABEL}", domain(user)?);
    // Clear any stale registration from a previous install before
    // re-bootstrapping with the (possibly updated) plist.
    let _ = launchctl(&["bootout", &target]);
    launchctl_checked(&["bootstrap", &domain(user)?, &path.to_string_lossy()])?;
    launchctl_checked(&["enable", &target])?;

    println!(
        "installed and started {SERVICE_NAME} ({} job at {})",
        if user { "user" } else { "system" },
        path.display()
    );
    Ok(())
}

pub fn uninstall(user: bool) -> anyhow::Result<()> {
    let path = plist_path(user)?;
    let target = format!("{}/{LABEL}", domain(user)?);
    let _ = launchctl(&["bootout", &target]);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| anyhow::anyhow!("removing {}: {e}", path.display()))?;
    }
    println!("uninstalled {SERVICE_NAME}");
    Ok(())
}

pub fn start(user: bool) -> anyhow::Result<()> {
    let target = format!("{}/{LABEL}", domain(user)?);
    launchctl_checked(&["kickstart", "-k", &target])?;
    println!("started {SERVICE_NAME}");
    Ok(())
}

pub fn stop(user: bool) -> anyhow::Result<()> {
    let target = format!("{}/{LABEL}", domain(user)?);
    launchctl_checked(&["stop", &target])?;
    println!("stopped {SERVICE_NAME}");
    Ok(())
}

pub fn status(user: bool) -> anyhow::Result<()> {
    let target = format!("{}/{LABEL}", domain(user)?);
    // Non-zero just means "not loaded" — informative, not a failure of
    // this command.
    let _ = launchctl(&["print", &target]);
    Ok(())
}
