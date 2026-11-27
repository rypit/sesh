//! `sesh feed add|remove|list` (specs/006 contracts/cli.md). The gateway
//! holds the feeds: these commands ask it, so it checks the address,
//! stores the feed and audits the change itself.

use crate::credential::{admin_token, admin_url};
use crate::host::http::Http;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::path::Path;

const UNREACHABLE: &str = "can't reach the gateway; run `sesh up`";

fn error_of(body: &Value) -> String {
    body["error"].as_str().unwrap_or("refused").to_string()
}

pub fn add(
    dir: &Path,
    http: &dyn Http,
    owner: &str,
    url: &str,
    name: Option<&str>,
) -> Result<String> {
    let token = admin_token(dir)?;
    let mut body = json!({"owner": owner, "url": url});
    if let Some(n) = name {
        body["name"] = json!(n);
    }
    let r = http
        .post(&format!("{}/feeds/add", admin_url()), Some(&token), &body)
        .context(UNREACHABLE)?;
    match r.status {
        200 => Ok(format!(
            "added feed {} for {owner}: {}",
            r.body["name"].as_str().unwrap_or("?"),
            r.body["url"].as_str().unwrap_or(url)
        )),
        404 => bail!("no user named {owner}; see `sesh user list`"),
        422 => bail!("refused: {}", error_of(&r.body)),
        s => bail!("the gateway answered {s}"),
    }
}

pub fn remove(dir: &Path, http: &dyn Http, owner: &str, name: &str) -> Result<String> {
    let token = admin_token(dir)?;
    let r = http
        .post(
            &format!("{}/feeds/remove", admin_url()),
            Some(&token),
            &json!({"owner": owner, "name": name}),
        )
        .context(UNREACHABLE)?;
    match r.status {
        200 => Ok(format!("removed feed {name} from {owner}")),
        404 => bail!("{owner} has no feed named {name}; see `sesh feed list`"),
        s => bail!("the gateway answered {s}"),
    }
}

/// Every feed (or one owner's): `owner  name  url  added-by  last-fetch`.
pub fn list(dir: &Path, http: &dyn Http, owner: Option<&str>) -> Result<Vec<String>> {
    let token = admin_token(dir)?;
    let r = http
        .get(&format!("{}/feeds", admin_url()), Some(&token))
        .context(UNREACHABLE)?;
    if r.status != 200 {
        bail!("the gateway answered {}", r.status);
    }
    if r.body["state"] != "ok" {
        bail!(
            "the feed service: {}",
            r.body["message"].as_str().unwrap_or("unavailable")
        );
    }
    let lines = r.body["feeds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| owner.is_none_or(|o| f["owner"] == o))
        .map(|f| {
            let last = match f["error"].as_str() {
                Some(e) => format!("failed: {e}"),
                None => "ok".into(),
            };
            format!(
                "{}  {}  {}  last update {last}",
                f["owner"].as_str().unwrap_or("?"),
                f["name"].as_str().unwrap_or("?"),
                f["url"].as_str().unwrap_or("?"),
            )
        })
        .collect();
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::fake::FakeHttp;
    use crate::secrets::Secrets;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("sesh-feed-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        crate::secrets::ensure_private_dir(&d).unwrap();
        let mut s = Secrets::default();
        s.fill_missing().unwrap();
        s.save(&d).unwrap();
        d
    }

    #[test]
    fn add_sends_owner_url_and_name_and_reports_refusals() {
        let d = dir("add");
        let http = FakeHttp::new(|_, url, body| {
            assert!(url.ends_with("/admin/feeds/add"));
            let b = body.unwrap();
            assert_eq!(
                (b["owner"].as_str(), b["name"].as_str()),
                (Some("ana"), Some("news"))
            );
            (
                200,
                json!({"name": "news", "url": "https://example.org/rss"}),
            )
        });
        let out = add(&d, &http, "ana", "https://example.org/rss", Some("news")).unwrap();
        assert_eq!(out, "added feed news for ana: https://example.org/rss");
        let refused = FakeHttp::new(|_, _, body| {
            assert!(
                body.unwrap().get("name").is_none(),
                "no name: the gateway picks one"
            );
            (
                422,
                json!({"error": "that address isn't on the public internet"}),
            )
        });
        let e = add(&d, &refused, "ana", "http://10.0.0.1/", None).unwrap_err();
        assert_eq!(
            e.to_string(),
            "refused: that address isn't on the public internet"
        );
        let none = FakeHttp::new(|_, _, _| (404, json!({})));
        assert!(
            add(&d, &none, "zed", "https://example.org/", None)
                .unwrap_err()
                .to_string()
                .contains("no user named zed")
        );
        assert!(
            remove(&d, &none, "ana", "news")
                .unwrap_err()
                .to_string()
                .contains("no feed named news")
        );
    }

    #[test]
    fn list_filters_by_owner_and_shows_failures() {
        let d = dir("list");
        let http = FakeHttp::new(|_, url, _| {
            assert!(url.ends_with("/admin/feeds"));
            (
                200,
                json!({"state": "ok", "feeds": [
                    {"owner": "ana", "name": "garden", "url": "https://example.org/g", "error": null},
                    {"owner": "household", "name": "harbour", "url": "https://example.org/h", "error": "the feed answered 500"}
                ]}),
            )
        });
        let all = list(&d, &http, None).unwrap();
        assert_eq!(all.len(), 2);
        assert!(all[0].ends_with("last update ok"));
        assert!(all[1].ends_with("last update failed: the feed answered 500"));
        let down = FakeHttp::new(|_, _, _| {
            (
                200,
                json!({"state": "unavailable", "message": "the feed service is unavailable"}),
            )
        });
        assert!(
            list(&d, &down, None)
                .unwrap_err()
                .to_string()
                .contains("unavailable")
        );
        let ana = list(&d, &http, Some("ana")).unwrap();
        assert_eq!(ana.len(), 1);
        assert!(ana[0].starts_with("ana  garden  https://example.org/g"));
    }
}
