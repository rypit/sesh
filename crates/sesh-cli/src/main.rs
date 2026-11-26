//! sesh admin CLI (specs/001-foundation/contracts/cli.md).
//!
//! Exit codes: 0 success; 1 a check or operation failed; 2 usage error
//! (from clap); 3 a prerequisite is missing.

mod config;
mod doctor;
mod host;
mod init;
mod render;
mod secrets;
mod up;

use anyhow::Result;
use clap::{Parser, Subcommand};
use host::compose::DockerCompose;
use host::http::UreqHttp;
use host::tailscale::TailscaleCli;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

#[derive(Parser)]
#[command(
    name = "sesh",
    version,
    about = "Operate the sesh household agent stack"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create or update sesh's config and secrets in the data directory.
    Init {
        /// Base URL of an OpenAI-compatible inference server, e.g. http://host:11434/v1
        #[arg(long)]
        inference_url: Option<String>,
        /// The server's model name behind the agent-main alias.
        #[arg(long)]
        model: Option<String>,
        /// Use the stub model server (development and CI).
        #[arg(long, conflicts_with_all = ["inference_url", "model"])]
        stub: bool,
        /// Email of the first Open WebUI admin account.
        #[arg(long)]
        admin_email: Option<String>,
    },
    /// Start or update the stack.
    Up,
    /// Stop the stack (data is kept).
    Down,
    /// Check that the stack is healthy and safe.
    Doctor {
        /// Print the checks as JSON.
        #[arg(long)]
        json: bool,
    },
}

fn compose_for(dir: &Path) -> Result<DockerCompose> {
    let config = config::Config::load(dir)?;
    let stub = init::is_stub(&config);
    let files = up::write_compose_files(dir, stub)?;
    let mut vars = vec![
        ("SESH_DATA_DIR".to_string(), dir.display().to_string()),
        (
            "SESH_WEBUI_PORT".to_string(),
            config.ports.webui.to_string(),
        ),
        (
            "SESH_ROUTER_PORT".to_string(),
            config.ports.router.to_string(),
        ),
    ];
    if let Some(name) = &config.inference.api_key_env
        && let Ok(value) = std::env::var(name)
    {
        vars.push(("SESH_INFERENCE_API_KEY".to_string(), value));
    }
    Ok(DockerCompose {
        project: "sesh".into(),
        files,
        env_file: dir.join(secrets::FILE_NAME),
        vars,
    })
}

fn run(cli: Cli) -> Result<ExitCode> {
    let dir = init::data_dir()?;
    match cli.command {
        Command::Init {
            inference_url,
            model,
            stub,
            admin_email,
        } => {
            let opts = init::InitOptions {
                inference_url,
                model,
                stub,
                admin_email,
            };
            for line in init::run(&dir, &opts)? {
                println!("{line}");
            }
        }
        Command::Up => {
            if !dir.join(config::FILE_NAME).exists() {
                eprintln!("no sesh config in {}; run `sesh init` first", dir.display());
                return Ok(ExitCode::from(3));
            }
            let compose = compose_for(&dir)?;
            let http = UreqHttp::new(Duration::from_secs(30));
            for line in up::run(&dir, &compose, &http)? {
                println!("{line}");
            }
        }
        Command::Down => up::down(&compose_for(&dir)?)?,
        Command::Doctor { json } => {
            let config = config::Config::load(&dir)?;
            let secrets = secrets::Secrets::load(&dir)?;
            let http = UreqHttp::new(Duration::from_secs(config.inference.timeout_secs.min(60)));
            let checks = doctor::run(&config, &secrets, &compose_for(&dir)?, &http, &TailscaleCli);
            if json {
                println!("{}", serde_json::to_string_pretty(&checks)?);
            } else {
                print!("{}", doctor::render_text(&checks));
            }
            if doctor::failed(&checks) {
                return Ok(ExitCode::from(1));
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}
