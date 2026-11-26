//! Seam: the only module that runs `docker compose`.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

/// A published port of a service, as Docker reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub address: String,
    pub host_port: u16,
    pub target_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceState {
    pub service: String,
    /// e.g. "running", "exited".
    pub state: String,
    /// e.g. "healthy", "starting", "unhealthy", or "" when there is no check.
    pub health: String,
    pub published: Vec<Published>,
}

pub trait Compose {
    fn up(&self, services: &[&str]) -> Result<()>;
    fn restart(&self, service: &str) -> Result<()>;
    fn down(&self, remove_volumes: bool) -> Result<()>;
    fn ps(&self) -> Result<Vec<ServiceState>>;
}

/// Runs `docker compose -p <project> --env-file <secrets> -f <files…>`.
pub struct DockerCompose {
    pub project: String,
    pub files: Vec<PathBuf>,
    pub env_file: PathBuf,
    /// Extra variables for interpolation (data dir, ports).
    pub vars: Vec<(String, String)>,
}

impl DockerCompose {
    fn command(&self) -> Command {
        let mut c = Command::new("docker");
        c.arg("compose").arg("-p").arg(&self.project);
        c.arg("--env-file").arg(&self.env_file);
        for f in &self.files {
            c.arg("-f").arg(f);
        }
        for (k, v) in &self.vars {
            c.env(k, v);
        }
        c
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        let out = self
            .command()
            .args(args)
            .output()
            .context("can't run `docker compose`; is Docker installed and running?")?;
        if !out.status.success() {
            bail!(
                "`docker compose {}` failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

impl Compose for DockerCompose {
    fn up(&self, services: &[&str]) -> Result<()> {
        let mut args = vec!["up", "-d", "--wait", "--remove-orphans"];
        args.extend_from_slice(services);
        self.run(&args).map(|_| ())
    }

    fn restart(&self, service: &str) -> Result<()> {
        self.run(&["up", "-d", "--wait", "--force-recreate", service])
            .map(|_| ())
    }

    fn down(&self, remove_volumes: bool) -> Result<()> {
        if remove_volumes {
            self.run(&["down", "-v"]).map(|_| ())
        } else {
            self.run(&["down"]).map(|_| ())
        }
    }

    fn ps(&self) -> Result<Vec<ServiceState>> {
        parse_ps(&self.run(&["ps", "--all", "--format", "json"])?)
    }
}

/// Parses `docker compose ps --format json`, which is either one JSON array
/// or one JSON object per line depending on the Compose version.
pub fn parse_ps(text: &str) -> Result<Vec<ServiceState>> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let items: Vec<Value> = if text.starts_with('[') {
        serde_json::from_str(text)?
    } else {
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()?
    };
    Ok(items.iter().map(service_state).collect())
}

fn service_state(v: &Value) -> ServiceState {
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let published = v
        .get("Publishers")
        .and_then(Value::as_array)
        .map(|ps| {
            ps.iter()
                .filter_map(|p| {
                    let host_port = p.get("PublishedPort")?.as_u64()? as u16;
                    if host_port == 0 {
                        return None; // exposed but not published
                    }
                    Some(Published {
                        address: p.get("URL")?.as_str()?.to_string(),
                        host_port,
                        target_port: p.get("TargetPort")?.as_u64()? as u16,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    ServiceState {
        service: s("Service"),
        state: s("State"),
        health: s("Health"),
        published,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lines_and_arrays() {
        let line = r#"{"Service":"open-webui","State":"running","Health":"healthy","Publishers":[{"URL":"127.0.0.1","TargetPort":8080,"PublishedPort":3000,"Protocol":"tcp"}]}
{"Service":"postgres","State":"running","Health":"healthy","Publishers":[{"URL":"","TargetPort":5432,"PublishedPort":0,"Protocol":"tcp"}]}"#;
        let a = parse_ps(line).unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].published[0].address, "127.0.0.1");
        assert_eq!(a[0].published[0].host_port, 3000);
        assert!(a[1].published.is_empty());
        let arr = format!("[{}]", line.replace('\n', ","));
        assert_eq!(parse_ps(&arr).unwrap(), a);
        assert!(parse_ps("").unwrap().is_empty());
    }
}
