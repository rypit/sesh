//! `sesh user add|remove|list` (specs/005 contracts/cli.md): household
//! users, each with their own approval topic and ntfy reader account. The
//! Household always exists and isn't listed here.

use crate::agent::gateway_dir;
use crate::secrets::{self, Secrets};
use anyhow::{Result, bail};
use sesh_core::{
    HOUSEHOLD, RESERVED_USER_NAMES, User, UsersFile, reserved_user_name, valid_user_name,
};
use std::path::Path;

/// `secrets.env` key for a user's ntfy password.
pub fn password_key(name: &str) -> String {
    format!(
        "NTFY_PASSWORD_{}",
        name.to_ascii_uppercase().replace('-', "_")
    )
}

pub fn load(dir: &Path) -> Result<UsersFile> {
    UsersFile::load(&gateway_dir(dir))
}

pub fn add(dir: &Path, name: &str) -> Result<Vec<String>> {
    if reserved_user_name(name) {
        bail!(
            "`{name}` is reserved (sesh uses it for {}); pick another user name",
            if name == HOUSEHOLD {
                "the shared Household"
            } else {
                "its own service accounts"
            }
        );
    }
    if !valid_user_name(name) {
        bail!(
            "user names are a-z, 0-9 and -, start with a letter, up to 32 characters, and can't be {}",
            RESERVED_USER_NAMES.join(" or ")
        );
    }
    let mut users = load(dir)?;
    if users.get(name).is_some() {
        bail!("user {name} already exists");
    }
    let mut b = [0u8; 4];
    getrandom::fill(&mut b).map_err(|e| anyhow::anyhow!("OS random source failed: {e}"))?;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let topic = format!("sesh-{name}-{hex}");
    users.users.push(User {
        name: name.into(),
        topic: topic.clone(),
    });
    std::fs::create_dir_all(gateway_dir(dir))?;
    users.save(&gateway_dir(dir))?;
    let mut s = Secrets::load(dir)?;
    s.set(&password_key(name), &secrets::random_token()?);
    s.save(dir)?;
    Ok(vec![
        format!("added user {name}; approvals go to ntfy topic {topic}"),
        format!(
            "they subscribe in the ntfy app as {} with the password {} in secrets.env (not shown); run `sesh up` to apply",
            crate::ntfy::reader_name(name),
            password_key(name)
        ),
    ])
}

pub fn remove(dir: &Path, name: &str) -> Result<String> {
    let mut users = load(dir)?;
    let before = users.users.len();
    users.users.retain(|u| u.name != name);
    if users.users.len() == before {
        bail!("no user named {name}");
    }
    users.save(&gateway_dir(dir))?;
    let mut s = Secrets::load(dir)?;
    s.remove(&password_key(name));
    s.save(dir)?;
    Ok(format!(
        "removed user {name}; their agents stop working. Run `sesh up` to update the approval channel."
    ))
}

pub fn list(dir: &Path) -> Result<Vec<String>> {
    Ok(load(dir)?
        .users
        .iter()
        .map(|u| format!("{}  {}", u.name, u.topic))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("sesh-user-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        crate::secrets::ensure_private_dir(&d).unwrap();
        d
    }

    #[test]
    fn add_list_remove() {
        let d = tempdir("alr");
        let out = add(&d, "ana").unwrap();
        assert!(out[0].contains("sesh-ana-"));
        let pw = Secrets::load(&d)
            .unwrap()
            .get("NTFY_PASSWORD_ANA")
            .unwrap()
            .to_string();
        assert!(
            !out.join(" ").contains(&pw),
            "the password is never printed"
        );
        assert!(add(&d, "ana").is_err());
        assert!(add(&d, "household").is_err());
        let e = add(&d, "gateway").unwrap_err().to_string();
        assert!(
            e.contains("gateway") && e.contains("reserved"),
            "a clear error: {e}"
        );
        assert!(add(&d, "Bad").is_err());
        assert_eq!(list(&d).unwrap().len(), 1);
        remove(&d, "ana").unwrap();
        assert!(list(&d).unwrap().is_empty());
        assert!(
            Secrets::load(&d)
                .unwrap()
                .get("NTFY_PASSWORD_ANA")
                .is_none()
        );
        assert!(remove(&d, "ana").is_err());
    }

    #[test]
    fn agents_need_a_known_owner() {
        let d = tempdir("owner");
        assert!(crate::agent::add(&d, "a1", &["search".into()], "ana", None).is_err());
        add(&d, "ana").unwrap();
        assert!(
            crate::agent::add(&d, "a1", &["search".into()], "ana", None)
                .unwrap()
                .contains("for ana")
        );
    }
}
