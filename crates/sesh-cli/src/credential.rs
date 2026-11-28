//! `sesh credential request|list` (specs/004 contracts/credentials.md): the
//! operator asks the gateway for a one-time form link; the person pastes
//! the value there. This command never sees, asks for or prints a value.

use crate::host::http::Http;
use crate::secrets::Secrets;
use anyhow::{Context, Result, bail};
use serde_json::json;
use std::path::Path;

pub fn admin_url() -> String {
    let port = std::env::var("SESH_APPROVALS_PORT").unwrap_or_else(|_| "7002".into());
    format!("http://127.0.0.1:{port}/admin")
}

pub fn admin_token(dir: &Path) -> Result<String> {
    Secrets::load(dir)?
        .get("SESH_GATEWAY_ADMIN_TOKEN")
        .map(str::to_string)
        .context("no gateway admin token; run `sesh init` and `sesh up`")
}

pub fn request(dir: &Path, http: &dyn Http, connector: &str, name: &str) -> Result<Vec<String>> {
    let token = admin_token(dir)?;
    let r = http
        .post(
            &format!("{}/forms", admin_url()),
            Some(&token),
            &json!({"connector": connector, "name": name}),
        )
        .context("can't reach the gateway; run `sesh up`")?;
    match r.status {
        200 => {}
        404 => bail!("{connector} declares no credential named {name}"),
        s => bail!("the gateway answered {s}"),
    }
    let url = r.body["url"]
        .as_str()
        .context("no link in the gateway's reply")?;
    let mins = r.body["expires_in_secs"].as_u64().unwrap_or(1800) / 60;
    Ok(vec![
        format!(
            "Open this link once, in a browser you trust, and paste the value of {name} for {connector}:"
        ),
        url.to_string(),
        format!("It works once and expires in {mins} minutes. sesh never shows the value."),
    ])
}

/// `sesh browser login <owner> <site>` (specs/010 R7): a one-time form for
/// an owner's login to one site; the agent can use it only on that site.
pub fn site_login(dir: &Path, http: &dyn Http, owner: &str, site: &str) -> Result<Vec<String>> {
    let token = admin_token(dir)?;
    let r = http
        .post(
            &format!("{}/browser/login-form", admin_url()),
            Some(&token),
            &json!({"owner": owner, "site": site}),
        )
        .context("can't reach the gateway; run `sesh up`")?;
    match r.status {
        200 => {}
        400 => bail!("give the site as a plain domain name, like example.org"),
        404 => {
            bail!("no user named {owner}; add them with `sesh user add {owner}` (or use household)")
        }
        s => bail!("the gateway answered {s}"),
    }
    let url = r.body["url"]
        .as_str()
        .context("no link in the gateway's reply")?;
    let mins = r.body["expires_in_secs"].as_u64().unwrap_or(1800) / 60;
    Ok(vec![
        format!(
            "Open this link once, in a browser you trust, and enter {owner}'s login for {site}:"
        ),
        url.to_string(),
        format!(
            "It works once and expires in {mins} minutes. The agent can use the login only on {site}, and never sees it."
        ),
    ])
}

pub fn list(dir: &Path, http: &dyn Http) -> Result<Vec<String>> {
    let token = admin_token(dir)?;
    let r = http
        .get(&format!("{}/credentials", admin_url()), Some(&token))
        .context("can't reach the gateway; run `sesh up`")?;
    if r.status != 200 {
        bail!("the gateway answered {}", r.status);
    }
    Ok(r.body
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            format!(
                "{}  {}  {}",
                c["connector"].as_str().unwrap_or("?"),
                c["name"].as_str().unwrap_or("?"),
                c["state"].as_str().unwrap_or("?")
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::fake::FakeHttp;
    use serde_json::json;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("sesh-cred-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        crate::secrets::ensure_private_dir(&d).unwrap();
        let mut s = Secrets::default();
        s.fill_missing().unwrap();
        s.save(&d).unwrap();
        d
    }

    #[test]
    fn request_prints_only_the_link() {
        let d = dir("request");
        let http = FakeHttp::new(|_, url, body| {
            assert!(url.ends_with("/admin/forms"));
            assert_eq!(body.unwrap()["name"], "TOKEN");
            (
                200,
                json!({"url": "http://127.0.0.1:7002/form/f1#sec", "expires_in_secs": 1800}),
            )
        });
        let out = request(&d, &http, "testcred", "TOKEN").unwrap();
        assert_eq!(out[1], "http://127.0.0.1:7002/form/f1#sec");
        assert!(out[2].contains("30 minutes"));
        let none = FakeHttp::new(|_, _, _| (404, json!({})));
        assert!(
            request(&d, &none, "search", "TOKEN")
                .unwrap_err()
                .to_string()
                .contains("declares no credential")
        );
    }

    #[test]
    fn list_shows_names_and_states() {
        let d = dir("list");
        let http = FakeHttp::new(|_, _, _| {
            (
                200,
                json!([{"connector": "testcred", "name": "TOKEN", "state": "missing"}]),
            )
        });
        assert_eq!(
            list(&d, &http).unwrap(),
            vec!["testcred  TOKEN  missing".to_string()]
        );
    }
}
