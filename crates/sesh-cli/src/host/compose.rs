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

/// What `remove_network_if_unused` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkRemoval {
    Removed,
    Absent,
    /// Still in use by these containers; left alone.
    InUse(Vec<String>),
}

pub trait Compose {
    fn up(&self, services: &[&str]) -> Result<()>;
    fn restart(&self, service: &str) -> Result<()>;
    fn down(&self, remove_volumes: bool) -> Result<()>;
    fn ps(&self) -> Result<Vec<ServiceState>>;
    /// Runs SQL in the postgres service as the database owner, on stdin
    /// (so nothing in it, such as a password, appears in a process list).
    /// Returns psql's unaligned output.
    fn exec_sql(&self, database: &str, sql: &str) -> Result<String>;
    /// The address ranges Docker assigned to one of the project's networks
    /// (`<project>_<network>`), in CIDR form (specs/008 research.md R10).
    fn network_subnets(&self, network: &str) -> Result<Vec<String>>;
    /// Removes a project network if nothing uses it.
    fn remove_network_if_unused(&self, network: &str) -> Result<NetworkRemoval>;
    /// Each running service's networks, by Compose network name (without
    /// the project prefix), for the doctor's layout check.
    fn service_networks(&self) -> Result<Vec<(String, Vec<String>)>>;
    /// The project's networks, without the project prefix (to remove a
    /// removed agent's networks: specs/009).
    fn project_networks(&self) -> Result<Vec<String>>;
    /// Runs a command in a running service's container and returns its
    /// standard output (the doctor's runtime check: specs/009).
    fn exec(&self, service: &str, args: &[&str]) -> Result<String>;
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

    fn docker(&self, args: &[&str]) -> Result<std::process::Output> {
        Command::new("docker")
            .args(args)
            .output()
            .context("can't run `docker`; is Docker installed and running?")
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

    fn exec_sql(&self, database: &str, sql: &str) -> Result<String> {
        use std::io::Write;
        let mut child = self
            .command()
            .args([
                "exec",
                "-T",
                "postgres",
                "psql",
                "-U",
                "sesh",
                "-d",
                database,
                "-At",
                "-v",
                "ON_ERROR_STOP=1",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("can't run `docker compose exec`")?;
        child
            .stdin
            .take()
            .context("no stdin")?
            .write_all(sql.as_bytes())?;
        let out = child.wait_with_output()?;
        if !out.status.success() {
            // stderr may echo SQL; report only its first line, never the input.
            let err = String::from_utf8_lossy(&out.stderr);
            bail!("psql failed: {}", err.lines().next().unwrap_or("").trim());
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn network_subnets(&self, network: &str) -> Result<Vec<String>> {
        let name = format!("{}_{network}", self.project);
        let out = self.docker(&[
            "network",
            "inspect",
            "-f",
            "{{range .IPAM.Config}}{{.Subnet}} {{end}}",
            &name,
        ])?;
        if !out.status.success() {
            bail!("can't inspect {name}");
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .map(str::to_string)
            .collect())
    }

    fn remove_network_if_unused(&self, network: &str) -> Result<NetworkRemoval> {
        let name = format!("{}_{network}", self.project);
        let out = self.docker(&[
            "network",
            "inspect",
            "-f",
            "{{range .Containers}}{{.Name}} {{end}}",
            &name,
        ])?;
        if !out.status.success() {
            return Ok(NetworkRemoval::Absent);
        }
        let users: Vec<String> = String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .map(str::to_string)
            .collect();
        if !users.is_empty() {
            return Ok(NetworkRemoval::InUse(users));
        }
        let rm = self.docker(&["network", "rm", &name])?;
        if !rm.status.success() {
            bail!(
                "can't remove {name}: {}",
                String::from_utf8_lossy(&rm.stderr).trim()
            );
        }
        Ok(NetworkRemoval::Removed)
    }

    fn service_networks(&self) -> Result<Vec<(String, Vec<String>)>> {
        let prefix = format!("{}_", self.project);
        let ids = self.run(&["ps", "-q"])?;
        let mut out = vec![];
        for id in ids.split_whitespace() {
            let o = self.docker(&[
                "inspect",
                "-f",
                "{{index .Config.Labels \"com.docker.compose.service\"}}|{{range $k, $v := .NetworkSettings.Networks}}{{$k}} {{end}}",
                id,
            ])?;
            if !o.status.success() {
                continue;
            }
            let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let Some((service, nets)) = text.split_once('|') else {
                continue;
            };
            let nets = nets
                .split_whitespace()
                .map(|n| n.strip_prefix(&prefix).unwrap_or(n).to_string())
                .collect();
            out.push((service.to_string(), nets));
        }
        out.sort();
        Ok(out)
    }

    fn project_networks(&self) -> Result<Vec<String>> {
        let prefix = format!("{}_", self.project);
        let label = format!("label=com.docker.compose.project={}", self.project);
        let out = self.docker(&["network", "ls", "--filter", &label, "--format", "{{.Name}}"])?;
        if !out.status.success() {
            bail!("can't list the project's networks");
        }
        Ok(String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .map(|n| n.strip_prefix(&prefix).unwrap_or(n).to_string())
            .collect())
    }

    fn exec(&self, service: &str, args: &[&str]) -> Result<String> {
        let mut a = vec!["exec", "-T", service];
        a.extend_from_slice(args);
        self.run(&a)
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
