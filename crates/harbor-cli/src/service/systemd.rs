use std::path::{Path, PathBuf};
use std::process::Command;

use harbor_core::SERVICE_NAME;

fn unit_path(user: bool) -> anyhow::Result<PathBuf> {
    if user {
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?;
        Ok(home.join(".config/systemd/user").join(format!("{SERVICE_NAME}.service")))
    } else {
        Ok(PathBuf::from("/etc/systemd/system").join(format!("{SERVICE_NAME}.service")))
    }
}

fn unit_contents(binary: &Path, user: bool) -> String {
    let wanted_by = if user { "default.target" } else { "multi-user.target" };
    format!(
        "[Unit]\n\
         Description=Harbor daemon\n\
         After=network.target\n\
         \n\
         [Service]\n\
         ExecStart={}\n\
         Restart=on-failure\n\
         RestartSec=2\n\
         \n\
         [Install]\n\
         WantedBy={wanted_by}\n",
        binary.display(),
    )
}

fn systemctl(user: bool, args: &[&str]) -> anyhow::Result<std::process::ExitStatus> {
    let mut cmd = Command::new("systemctl");
    if user {
        cmd.arg("--user");
    }
    cmd.args(args);
    cmd.status()
        .map_err(|e| anyhow::anyhow!("running `systemctl{}`: {e}", if user { " --user" } else { "" }))
}

fn systemctl_checked(user: bool, args: &[&str]) -> anyhow::Result<()> {
    let status = systemctl(user, args)?;
    if !status.success() {
        anyhow::bail!("systemctl {} exited with {status}", args.join(" "));
    }
    Ok(())
}

pub fn install(user: bool, binary: &Path) -> anyhow::Result<()> {
    let path = unit_path(user)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("creating {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, unit_contents(binary, user)).map_err(|e| {
        anyhow::anyhow!(
            "writing {}: {e}{}",
            path.display(),
            if user { "" } else { " (a system-wide install needs root — re-run with sudo, or pass --user)" }
        )
    })?;

    systemctl_checked(user, &["daemon-reload"])?;
    systemctl_checked(user, &["enable", "--now", SERVICE_NAME])?;

    println!(
        "installed and started {SERVICE_NAME} ({} unit at {})",
        if user { "user" } else { "system" },
        path.display()
    );
    if user {
        println!("note: a user-level service only runs while you're logged in, unless you also run:");
        println!("  loginctl enable-linger $USER");
    }
    Ok(())
}

pub fn uninstall(user: bool) -> anyhow::Result<()> {
    let path = unit_path(user)?;
    // Best-effort: fine if it's already stopped/disabled.
    let _ = systemctl(user, &["disable", "--now", SERVICE_NAME]);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| anyhow::anyhow!("removing {}: {e}", path.display()))?;
    }
    systemctl_checked(user, &["daemon-reload"])?;
    println!("uninstalled {SERVICE_NAME}");
    Ok(())
}

pub fn start(user: bool) -> anyhow::Result<()> {
    systemctl_checked(user, &["start", SERVICE_NAME])?;
    println!("started {SERVICE_NAME}");
    Ok(())
}

pub fn stop(user: bool) -> anyhow::Result<()> {
    systemctl_checked(user, &["stop", SERVICE_NAME])?;
    println!("stopped {SERVICE_NAME}");
    Ok(())
}

pub fn status(user: bool) -> anyhow::Result<()> {
    // `systemctl status` exits non-zero for an installed-but-inactive
    // service — that's informative output, not a failure of this command.
    systemctl(user, &["status", SERVICE_NAME, "--no-pager"])?;
    Ok(())
}
