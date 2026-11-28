//! `sesh agent add/remove/list` (specs/002-gateway-core/contracts/cli.md).
//!
//! The agent's token is generated here, stored only in
//! `$SESH_DATA_DIR/agents/<name>.token` (0600) for the agent's container,
//! and never printed. The gateway sees only its SHA-256, in
//! `$SESH_DATA_DIR/gateway/agents.toml`.

use crate::secrets::{self, Secrets, ensure_private_dir, random_token};
use anyhow::{Result, bail};
use sesh_core::{Agent, AgentsFile, token_hash, valid_agent_name};
use std::path::{Path, PathBuf};

pub const KNOWN_CONNECTORS: [&str; 7] = [
    "search", "geo", "mail", "calendar", "contacts", "docs", "feeds",
];
/// Test-only connectors (specs/004 research.md R7), allowed in stub mode only.
pub const TEST_CONNECTORS: [&str; 2] = ["testgate", "testcred"];
/// The agent runtimes sesh can run (specs/009).
pub const RUNTIMES: [&str; 1] = ["hermes"];

fn stub_mode(dir: &Path) -> bool {
    crate::config::Config::load(dir)
        .map(|c| crate::init::is_stub(&c))
        .unwrap_or(false)
}

pub fn gateway_dir(dir: &Path) -> PathBuf {
    dir.join("gateway")
}

pub fn token_path(dir: &Path, name: &str) -> PathBuf {
    dir.join("agents").join(format!("{name}.token"))
}

pub fn load(dir: &Path) -> Result<AgentsFile> {
    AgentsFile::load(&gateway_dir(dir))
}

pub fn add(
    dir: &Path,
    name: &str,
    connectors: &[String],
    owner: &str,
    runtime: Option<&str>,
) -> Result<String> {
    if let Some(r) = runtime
        && !RUNTIMES.contains(&r)
    {
        bail!("unknown runtime {r:?}; the only one is hermes");
    }
    let users = sesh_core::UsersFile::load(&gateway_dir(dir))?;
    if !users.knows(owner) {
        bail!("no user named {owner}; add them with `sesh user add {owner}` (or use household)");
    }
    if !valid_agent_name(name) {
        bail!("agent names are 1-32 characters of a-z, 0-9 and - (not starting with -)");
    }
    if connectors.is_empty() {
        bail!(
            "give at least one connector: {}",
            KNOWN_CONNECTORS.join(", ")
        );
    }
    let test_ok = stub_mode(dir);
    for c in connectors {
        if !KNOWN_CONNECTORS.contains(&c.as_str())
            && !(test_ok && TEST_CONNECTORS.contains(&c.as_str()))
        {
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
        runtime: runtime.map(str::to_string),
        owner: owner.to_string(),
    });
    std::fs::create_dir_all(gateway_dir(dir))?;
    file.save(&gateway_dir(dir))?;
    let mut msg = format!(
        "added agent {name} for {owner} ({}); token in {} (not shown). Run `sesh up` to start its executor.",
        connectors.join(", "),
        path.display()
    );
    if let Some(r) = runtime {
        // The key Open WebUI sends to the runtime (specs/009 research.md R7).
        let mut s = Secrets::load(dir)?;
        s.set(
            &secrets::agent_key(name, secrets::API_KEY),
            &random_token()?,
        );
        s.save(dir)?;
        msg.push_str(&format!(
            "\nit will run as a {r} agent after the next `sesh up`"
        ));
    }
    Ok(msg)
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
    let mut s = Secrets::load(dir)?;
    let api = secrets::agent_key(name, secrets::API_KEY);
    if s.get(&api).is_some() {
        // The router key is revoked in LiteLLM by the next `sesh up`,
        // which still needs it to find the key; only the API key goes now.
        s.remove(&api);
        s.save(dir)?;
    }
    Ok(format!(
        "removed agent {name}; its token no longer works. Run `sesh up` to remove its executor."
    ))
}

pub fn list(dir: &Path) -> Result<Vec<String>> {
    Ok(load(dir)?
        .agents
        .iter()
        .map(|a| {
            format!(
                "{}  {}  {}  {}",
                a.name,
                a.owner,
                a.connectors.join(","),
                a.runtime.as_deref().unwrap_or("-")
            )
        })
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
        let msg = add(
            &dir,
            "alice",
            &["search".into(), "geo".into()],
            "household",
            None,
        )
        .unwrap();
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
        assert!(
            add(&dir, "alice", &["search".into()], "household", None).is_err(),
            "duplicate"
        );
    }

    #[test]
    fn validates_names_and_connectors() {
        let dir = tempdir("val");
        assert!(add(&dir, "Bad Name", &["search".into()], "household", None).is_err());
        assert!(add(&dir, "bob", &[], "household", None).is_err());
        assert!(add(&dir, "bob", &["email".into()], "household", None).is_err());
    }

    #[test]
    fn a_runtime_agent_gets_an_api_key_and_loses_it_on_removal() {
        let dir = tempdir("rt");
        assert!(
            add(&dir, "h", &["search".into()], "household", Some("other"))
                .unwrap_err()
                .to_string()
                .contains("the only one is hermes")
        );
        let msg = add(
            &dir,
            "house-hold",
            &["search".into()],
            "household",
            Some("hermes"),
        )
        .unwrap();
        assert!(
            msg.contains("hermes agent after the next `sesh up`"),
            "{msg}"
        );
        let key = "AGENT_HOUSE_HOLD_API_KEY";
        let v = Secrets::load(&dir).unwrap().get(key).unwrap().to_string();
        assert!(v.len() >= 32 && !msg.contains(&v));
        assert_eq!(
            load(&dir)
                .unwrap()
                .get("house-hold")
                .unwrap()
                .runtime
                .as_deref(),
            Some("hermes")
        );
        assert!(list(&dir).unwrap()[0].ends_with("  hermes"));
        remove(&dir, "house-hold").unwrap();
        assert!(Secrets::load(&dir).unwrap().get(key).is_none());
    }

    #[test]
    fn remove_revokes() {
        let dir = tempdir("rm");
        add(&dir, "bob", &["search".into()], "household", None).unwrap();
        let token = std::fs::read_to_string(token_path(&dir, "bob")).unwrap();
        remove(&dir, "bob").unwrap();
        assert!(load(&dir).unwrap().by_token(&token).is_none());
        assert!(!token_path(&dir, "bob").exists());
        assert!(remove(&dir, "bob").is_err());
        assert!(list(&dir).unwrap().is_empty());
    }
}
