mod client;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use harbor_core::{AddAppRequest, AppState, GlobalConfig, HarborPaths, RestartPolicy, RuntimeKind};

use client::Client;

/// Harbor: a cross-platform app host (process supervision + reverse proxy).
#[derive(Parser)]
#[command(name = "harbor", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Add an app from a project directory (FR16).
    Add {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        runtime: Option<String>,
        #[arg(long = "command")]
        command: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long, value_enum, default_value = "on-failure")]
        restart: RestartArg,
    },
    /// Start a stopped app.
    Start { app: String },
    /// Stop a running app.
    Stop { app: String },
    /// Restart an app.
    Restart { app: String },
    /// Show status for one app, or all apps (FR18).
    Status { app: Option<String> },
    /// Show recent logs for an app (FR19).
    Logs {
        app: String,
        #[arg(short = 'f', long)]
        follow: bool,
        #[arg(long, default_value_t = 100)]
        lines: usize,
    },
    /// Remove an app (stops it first) (FR20).
    Remove { app: String },
}

#[derive(Clone, clap::ValueEnum)]
enum RestartArg {
    Never,
    OnFailure,
    Always,
}

impl From<RestartArg> for RestartPolicy {
    fn from(v: RestartArg) -> Self {
        match v {
            RestartArg::Never => RestartPolicy::Never,
            RestartArg::OnFailure => RestartPolicy::OnFailure,
            RestartArg::Always => RestartPolicy::Always,
        }
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let paths = HarborPaths::discover();
    let global = GlobalConfig::load_or_default(&paths.global_config_file)?;
    let token = std::fs::read_to_string(&paths.token_file)
        .map(|s| s.trim().to_string())
        .map_err(|_| {
            anyhow::anyhow!(
                "no daemon token found at {}; is the harbor daemon running?",
                paths.token_file.display()
            )
        })?;
    let client = Client::new(format!("http://{}", global.bind_addr), token);

    match cli.command {
        Command::Add {
            path,
            name,
            runtime,
            command,
            port,
            domain,
            restart,
        } => {
            let path = std::fs::canonicalize(&path)
                .map_err(|e| anyhow::anyhow!("invalid path {}: {e}", path.display()))?;
            let runtime = runtime.map(|r| r.parse::<RuntimeKind>()).transpose()?;
            let command = command.map(|c| c.split_whitespace().map(str::to_string).collect());
            let req = AddAppRequest {
                path,
                name,
                runtime,
                command,
                port,
                domain,
                env: Default::default(),
                restart_policy: Some(restart.into()),
            };
            let status = client.add_app(&req).await?;
            println!("added app '{}' (runtime: {})", status.name, status.runtime);
        }
        Command::Start { app } => {
            client.start_app(&app).await?;
            println!("started '{app}'");
        }
        Command::Stop { app } => {
            client.stop_app(&app).await?;
            println!("stopped '{app}'");
        }
        Command::Restart { app } => {
            client.restart_app(&app).await?;
            println!("restarted '{app}'");
        }
        Command::Status { app } => match app {
            Some(name) => print_status(&[client.get_app(&name).await?]),
            None => print_status(&client.list_apps().await?),
        },
        Command::Logs { app, follow, lines } => {
            let mut seen_out = 0usize;
            let mut seen_err = 0usize;
            loop {
                let logs = client.logs(&app, lines.max(1)).await?;
                // Polling tail: prints only lines beyond what was already
                // shown. If the window rolls over faster than the poll
                // interval, older new lines can be missed — acceptable for
                // an MVP log viewer (FR6/FR23 want a full streaming view).
                for line in logs.stdout.iter().skip(seen_out) {
                    println!("{line}");
                }
                for line in logs.stderr.iter().skip(seen_err) {
                    eprintln!("{line}");
                }
                seen_out = logs.stdout.len();
                seen_err = logs.stderr.len();
                if !follow {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
        Command::Remove { app } => {
            client.remove_app(&app).await?;
            println!("removed '{app}'");
        }
    }
    Ok(())
}

fn print_status(apps: &[harbor_core::AppStatus]) {
    if apps.is_empty() {
        println!("no apps registered");
        return;
    }
    println!(
        "{:<20} {:<8} {:<11} {:<8} {:<6} {:<9} {:<20}",
        "NAME", "RUNTIME", "STATE", "PID", "PORT", "RESTARTS", "DOMAIN"
    );
    for app in apps {
        let state_marker = match app.state {
            AppState::Running => "running",
            AppState::Stopped => "stopped",
            AppState::Restarting => "restarting",
            AppState::Crashed => "crashed",
        };
        println!(
            "{:<20} {:<8} {:<11} {:<8} {:<6} {:<9} {:<20}",
            app.name,
            app.runtime.as_str(),
            state_marker,
            app.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
            app.port.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
            app.restart_count,
            app.domain.clone().unwrap_or_else(|| "-".into()),
        );
    }
}

