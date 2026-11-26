//! `sesh agent add/remove/list` (specs/002-gateway-core/contracts/cli.md).
//!
//! The agent's token is generated here, stored only in
//! `$SESH_DATA_DIR/agents/<name>.token` (0600) for the agent's container,
//! and never printed. The gateway sees only its SHA-256, in
//! `$SESH_DATA_DIR/gateway/agents.toml`.

use crate::secrets::{ensure_private_dir, random_token};
use anyhow::{Result, bail};
use sesh_core::{Agent, AgentsFile, token_hash, valid_agent_name};
use std::path::{Path, PathBuf};

pub const KNOWN_CONNECTORS: [&str; 2] = ["search", "geo"];

pub fn gateway_dir(dir: &Path) -> PathBuf {
    dir.join("gateway")
}

pub fn token_path(dir: &Path, name: &str) -> PathBuf {
    dir.join("agents").join(format!("{name}.token"))
}

pub fn load(dir: &Path) -> Result<AgentsFile> {
    AgentsFile::load(&gateway_dir(dir))
}

pub fn add(dir: &Path, name: &str, connectors: &[String]) -> Result<String> {
    if !valid_agent_name(name) {
        bail!("agent names are 1-32 characters of a-z, 0-9 and - (not starting with -)");
    }
    if connectors.is_empty() {
        bail!(
            "give at least one connector: {}",
            KNOWN_CONNECTORS.join(", ")
        );
    }
    for c in connectors {
        if !KNOWN_CONNECTORS.contains(&c.as_str()) {
            bail!(
                "unknown connector {c:?}; known: {}",
                KNOWN_CONNECTORS.join(", ")
            );
        }
    }
    let mut file = load(dir)?;
    if file.get(name).is_some() {
        bail!("agent {name} already exists; remove it first to change it");
    }
    let token = random_token()?;
    ensure_private_dir(&dir.join("agents"))?;
    let path = token_path(dir, name);
    std::fs::write(&path, &token)?;
    restrict(&path)?;
    file.agents.push(Agent {
        name: name.to_string(),
        token_sha256: token_hash(&token),
        connectors: connectors.to_vec(),
    });
    std::fs::create_dir_all(gateway_dir(dir))?;
    file.save(&gateway_dir(dir))?;
    Ok(format!(
        "added agent {name} ({}); token in {} (not shown). Run `sesh up` to start its executor.",
        connectors.join(", "),
        path.display()
    ))
}

pub fn remove(dir: &Path, name: &str) -> Result<String> {
    let mut file = load(dir)?;
    let before = file.agents.len();
    file.agents.retain(|a| a.name != name);
    if file.agents.len() == before {
        bail!("no agent named {name}");
    }
    file.save(&gateway_dir(dir))?;
    let _ = std::fs::remove_file(token_path(dir, name));
    Ok(format!(
        "removed agent {name}; its token no longer works. Run `sesh up` to remove its executor."
    ))
}

pub fn list(dir: &Path) -> Result<Vec<String>> {
    Ok(load(dir)?
        .agents
        .iter()
        .map(|a| format!("{}  {}", a.name, a.connectors.join(",")))
        .collect())
}

#[cfg(unix)]
fn restrict(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("sesh-agent-{name}-{}", std::process::id()))
    }

    #[test]
    fn add_stores_only_the_hash_for_the_gateway() {
        let dir = tempdir("add");
        let msg = add(&dir, "alice", &["search".into(), "geo".into()]).unwrap();
        let token = std::fs::read_to_string(token_path(&dir, "alice")).unwrap();
        assert!(!msg.contains(&token), "the token must never be printed");
        let gw = std::fs::read_to_string(gateway_dir(&dir).join("agents.toml")).unwrap();
        assert!(!gw.contains(&token));
        assert_eq!(load(&dir).unwrap().by_token(&token).unwrap().name, "alice");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(token_path(&dir, "alice"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(add(&dir, "alice", &["search".into()]).is_err(), "duplicate");
    }

    #[test]
    fn validates_names_and_connectors() {
        let dir = tempdir("val");
        assert!(add(&dir, "Bad Name", &["search".into()]).is_err());
        assert!(add(&dir, "bob", &[]).is_err());
        assert!(add(&dir, "bob", &["email".into()]).is_err());
    }

    #[test]
    fn remove_revokes() {
        let dir = tempdir("rm");
        add(&dir, "bob", &["search".into()]).unwrap();
        let token = std::fs::read_to_string(token_path(&dir, "bob")).unwrap();
        remove(&dir, "bob").unwrap();
        assert!(load(&dir).unwrap().by_token(&token).is_none());
        assert!(!token_path(&dir, "bob").exists());
        assert!(remove(&dir, "bob").is_err());
        assert!(list(&dir).unwrap().is_empty());
    }
}
