//! sesh admin CLI (specs/001-foundation/contracts/cli.md).
//!
//! Exit codes: 0 success; 1 a check or operation failed; 2 usage error
//! (from clap); 3 a prerequisite is missing.

mod agent;
mod config;
mod credential;
mod doctor;
mod eval;
mod host;
mod infisical;
mod init;
mod ntfy;
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
        /// Home location for the geo connector, as LAT,LON.
        #[arg(long, value_parser = parse_home)]
        home: Option<(f64, f64)>,
    },
    /// Check that a model can use sesh's resources (runs the eval suite).
    #[command(args_conflicts_with_subcommands = true)]
    Eval {
        #[command(subcommand)]
        command: Option<EvalCommand>,
        /// The router alias to test.
        #[arg(long, default_value = "agent-main")]
        alias: String,
        /// Tasks that must pass (default: all).
        #[arg(long)]
        threshold: Option<usize>,
        /// Model turns per task (default 8).
        #[arg(long)]
        turns: Option<u32>,
        /// Tool calls per task (default 6).
        #[arg(long)]
        tool_calls: Option<u32>,
        /// Seconds per task (default 120).
        #[arg(long)]
        timeout: Option<u64>,
        /// Print the run as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Ask for a credential through a one-time form, or list credentials.
    Credential {
        #[command(subcommand)]
        command: CredentialCommand,
    },
    /// Manage the agents the gateway accepts.
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
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

#[derive(Subcommand)]
enum CredentialCommand {
    /// Print a one-time link where the person pastes a credential (never shown here).
    Request { connector: String, name: String },
    /// List the credentials connectors need, and whether each is set.
    List,
}

#[derive(Subcommand)]
enum EvalCommand {
    /// List saved runs, newest first.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Compare two runs task by task (default: the latest two).
    Compare {
        a: Option<String>,
        b: Option<String>,
    },
    /// Show one task's transcript (untrusted model output).
    Show { run: String, task: String },
}

#[derive(Subcommand)]
enum AgentCommand {
    /// Add an agent and generate its token (never printed).
    Add {
        name: String,
        /// Connectors the agent may use, comma-separated (search, geo).
        #[arg(long, value_delimiter = ',', required = true)]
        connectors: Vec<String>,
        /// Whose agent this is: a user (`sesh user add`), or household.
        #[arg(long, default_value = "household")]
        owner: String,
    },
    /// Remove an agent; its token stops working at once.
    Remove { name: String },
    /// List agents and their connectors.
    List,
}

fn parse_home(s: &str) -> Result<(f64, f64), String> {
    let (lat, lon) = s.split_once(',').ok_or("expected LAT,LON")?;
    let lat: f64 = lat.trim().parse().map_err(|_| "bad latitude")?;
    let lon: f64 = lon.trim().parse().map_err(|_| "bad longitude")?;
    Ok((lat, lon))
}

fn compose_for(dir: &Path) -> Result<DockerCompose> {
    let config = config::Config::load(dir)?;
    let files = up::write_compose_files(dir, &config)?;
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
    for var in [
        "SESH_GATEWAY_PORT",
        "SESH_GATEWAY_IMAGE",
        "SESH_EXECUTOR_IMAGE",
        "STUB_SCRIPT",
        "STUB_TOOLS",
        "STUB_TESTCRED_SHA256",
        "SESH_APPROVALS_PORT",
        "SESH_INFISICAL_PORT",
        "SESH_NTFY_PORT",
    ] {
        if let Ok(value) = std::env::var(var) {
            vars.push((var.to_string(), value));
        }
    }
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
            home,
        } => {
            let opts = init::InitOptions {
                inference_url,
                model,
                stub,
                admin_email,
                home,
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
        Command::Eval {
            command,
            alias,
            threshold,
            turns,
            tool_calls,
            timeout,
            json,
        } => {
            let opts = eval::Options {
                alias,
                threshold,
                turns,
                tool_calls,
                timeout,
                json,
            };
            return eval_command(&dir, command, opts);
        }
        Command::Credential { command } => {
            let http = UreqHttp::new(Duration::from_secs(15));
            let lines = match command {
                CredentialCommand::Request { connector, name } => {
                    credential::request(&dir, &http, &connector, &name)?
                }
                CredentialCommand::List => credential::list(&dir, &http)?,
            };
            for line in lines {
                println!("{line}");
            }
        }
        Command::Agent { command } => match command {
            AgentCommand::Add {
                name,
                connectors,
                owner,
            } => println!("{}", agent::add(&dir, &name, &connectors, &owner)?),
            AgentCommand::Remove { name } => println!("{}", agent::remove(&dir, &name)?),
            AgentCommand::List => {
                for line in agent::list(&dir)? {
                    println!("{line}");
                }
            }
        },
        Command::Down => up::down(&compose_for(&dir)?)?,
        Command::Doctor { json } => {
            let config = config::Config::load(&dir)?;
            let secrets = secrets::Secrets::load(&dir)?;
            let http = UreqHttp::new(Duration::from_secs(config.inference.timeout_secs.min(60)));
            let agents: Vec<String> = agent::load(&dir)?
                .agents
                .into_iter()
                .map(|a| a.name)
                .collect();
            let checks = doctor::run(
                &config,
                &secrets,
                &agents,
                &compose_for(&dir)?,
                &http,
                &TailscaleCli,
            );
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

fn eval_command(dir: &Path, command: Option<EvalCommand>, opts: eval::Options) -> Result<ExitCode> {
    match command {
        None => match eval::run(dir, &opts) {
            Ok(true) => Ok(ExitCode::SUCCESS),
            Ok(false) => Ok(ExitCode::from(1)),
            Err(e) if e.downcast_ref::<eval::Prerequisite>().is_some() => {
                eprintln!("{e}");
                Ok(ExitCode::from(3))
            }
            Err(e) => Err(e),
        },
        Some(EvalCommand::List { json }) => {
            let runs = eval::store::list(dir)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&runs)?);
            } else {
                for line in eval::store::list_lines(&runs) {
                    println!("{line}");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Some(EvalCommand::Compare { a, b }) => {
            let complete: Vec<_> = eval::store::list(dir)?
                .into_iter()
                .filter(|r| r.complete)
                .collect();
            let pick = |id: &str| eval::store::load(dir, id);
            let (ra, rb) = match (a, b) {
                (Some(a), Some(b)) => (pick(&a)?, pick(&b)?),
                (Some(a), None) => {
                    let latest = complete
                        .first()
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("no complete runs"))?;
                    (pick(&a)?, latest)
                }
                (None, _) => {
                    if complete.len() < 2 {
                        anyhow::bail!("need two complete runs to compare");
                    }
                    (complete[1].clone(), complete[0].clone())
                }
            };
            for line in eval::store::compare(&ra, &rb)? {
                println!("{line}");
            }
            Ok(ExitCode::SUCCESS)
        }
        Some(EvalCommand::Show { run, task }) => {
            print!("{}", eval::store::show(dir, &run, &task)?);
            Ok(ExitCode::SUCCESS)
        }
    }
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
