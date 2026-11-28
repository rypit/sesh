//! `sesh memory list|forget` (specs/011 contracts/memory-connector.md).
//! The gateway holds the memories: these commands ask its admin API, so
//! it checks the owner and audits the change itself.

use crate::credential::{admin_token, admin_url};
use crate::host::http::Http;
use anyhow::{Context, Result, bail};
use serde_json::json;
use std::path::Path;

const UNREACHABLE: &str = "can't reach the gateway; run `sesh up`";

/// One owner's facts, newest first: `id  updated  fact`.
pub fn list(dir: &Path, http: &dyn Http, owner: &str, limit: u32) -> Result<Vec<String>> {
    let token = admin_token(dir)?;
    let r = http
        .get(
            &format!("{}/memory?owner={owner}&limit={limit}", admin_url()),
            Some(&token),
        )
        .context(UNREACHABLE)?;
    match r.status {
        200 => {}
        404 => bail!("no user named {owner}; see `sesh user list`"),
        s => bail!("the gateway answered {s}"),
    }
    let facts = r.body["facts"].as_array().cloned().unwrap_or_default();
    if facts.is_empty() {
        return Ok(vec![format!("{owner} has no memories")]);
    }
    Ok(facts
        .iter()
        .map(|f| {
            format!(
                "{}  {}  {}",
                f["id"].as_str().unwrap_or("?"),
                f["updated"].as_str().unwrap_or("?"),
                f["fact"].as_str().unwrap_or("")
            )
        })
        .collect())
}

/// Forgets one fact, or all of an owner's (`id` is `None`).
pub fn forget(dir: &Path, http: &dyn Http, owner: &str, id: Option<&str>) -> Result<String> {
    let token = admin_token(dir)?;
    let body = match id {
        Some(id) => json!({"owner": owner, "id": id}),
        None => json!({"owner": owner, "all": true}),
    };
    let r = http
        .post(
            &format!("{}/memory/forget", admin_url()),
            Some(&token),
            &body,
        )
        .context(UNREACHABLE)?;
    match r.status {
        200 => {}
        404 => bail!("no user named {owner}; see `sesh user list`"),
        s => bail!("the gateway answered {s}"),
    }
    let n = r.body["forgotten"].as_u64().unwrap_or(0);
    match (id, n) {
        (Some(id), 0) => bail!("{owner} has no memory {id}; see `sesh memory list {owner}`"),
        (Some(id), _) => Ok(format!("forgot {id} for {owner}")),
        (None, n) => Ok(format!("forgot {n} memories for {owner}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::fake::FakeHttp;
    use crate::secrets::Secrets;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("sesh-memory-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        crate::secrets::ensure_private_dir(&d).unwrap();
        let mut s = Secrets::default();
        s.fill_missing().unwrap();
        s.save(&d).unwrap();
        d
    }

    #[test]
    fn list_shows_one_owners_facts() {
        let d = dir("list");
        let http = FakeHttp::new(|_, url, _| {
            assert!(url.ends_with("/admin/memory?owner=ana&limit=50"), "{url}");
            (
                200,
                json!({"facts": [{"id": "f1", "updated": "2026-11-28", "fact": "likes tea"}]}),
            )
        });
        assert_eq!(
            list(&d, &http, "ana", 50).unwrap(),
            vec!["f1  2026-11-28  likes tea"]
        );
        let empty = FakeHttp::new(|_, _, _| (200, json!({"facts": []})));
        assert_eq!(
            list(&d, &empty, "ana", 50).unwrap(),
            vec!["ana has no memories"]
        );
        let none = FakeHttp::new(|_, _, _| (404, json!({})));
        assert!(
            list(&d, &none, "zed", 50)
                .unwrap_err()
                .to_string()
                .contains("no user named zed")
        );
    }

    #[test]
    fn forget_sends_one_id_or_all() {
        let d = dir("forget");
        let one = FakeHttp::new(|_, url, body| {
            assert!(url.ends_with("/admin/memory/forget"));
            assert_eq!(body.unwrap()["id"], "f1");
            (200, json!({"forgotten": 1}))
        });
        assert_eq!(
            forget(&d, &one, "ana", Some("f1")).unwrap(),
            "forgot f1 for ana"
        );
        let all = FakeHttp::new(|_, _, body| {
            assert_eq!(body.unwrap()["all"], true);
            (200, json!({"forgotten": 3}))
        });
        assert_eq!(
            forget(&d, &all, "ana", None).unwrap(),
            "forgot 3 memories for ana"
        );
        let missing = FakeHttp::new(|_, _, _| (200, json!({"forgotten": 0})));
        assert!(
            forget(&d, &missing, "ana", Some("f9"))
                .unwrap_err()
                .to_string()
                .contains("no memory f9")
        );
    }
}
