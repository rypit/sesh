//! `sesh google connect|disconnect` (specs/005 contracts/cli.md): the
//! operator asks the gateway for a one-time consent link; the person opens
//! it and signs in to Google. No grant ever passes through this command.

use crate::credential::{admin_token, admin_url};
use crate::host::http::Http;
use anyhow::{Context, Result, bail};
use serde_json::json;
use std::path::Path;

pub fn connect(dir: &Path, http: &dyn Http, owner: &str) -> Result<Vec<String>> {
    let token = admin_token(dir)?;
    let r = http
        .post(
            &format!("{}/google/consent", admin_url()),
            Some(&token),
            &json!({"owner": owner}),
        )
        .context("can't reach the gateway; run `sesh up`")?;
    match r.status {
        200 => {}
        404 => bail!("no user named {owner}; see `sesh user list`"),
        s => bail!("the gateway answered {s}"),
    }
    let url = r.body["url"]
        .as_str()
        .context("no link in the gateway's reply")?;
    let mut out = vec![
        format!(
            "Have {owner} open this link once, in a browser on the tailnet, and sign in to Google:"
        ),
        url.to_string(),
        "It works once and expires in 30 minutes.".to_string(),
    ];
    if let Some(cb) = r.body["redirect_uri"].as_str() {
        out.push(format!(
            "The Google OAuth client must list {cb} as an authorized redirect URI."
        ));
    }
    Ok(out)
}

pub fn disconnect(dir: &Path, http: &dyn Http, owner: &str) -> Result<String> {
    let token = admin_token(dir)?;
    let r = http
        .post(
            &format!("{}/google/disconnect", admin_url()),
            Some(&token),
            &json!({"owner": owner}),
        )
        .context("can't reach the gateway; run `sesh up`")?;
    match r.status {
        200 => Ok(format!(
            "disconnected Google for {owner}; its grant is revoked and removed"
        )),
        404 => bail!("no user named {owner}; see `sesh user list`"),
        s => bail!("the gateway answered {s}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::fake::FakeHttp;
    use crate::secrets::Secrets;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("sesh-google-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        crate::secrets::ensure_private_dir(&d).unwrap();
        let mut s = Secrets::default();
        s.fill_missing().unwrap();
        s.save(&d).unwrap();
        d
    }

    #[test]
    fn connect_prints_the_link_and_the_callback() {
        let d = dir("connect");
        let http = FakeHttp::new(|_, url, body| {
            assert!(url.ends_with("/admin/google/consent"));
            assert_eq!(body.unwrap()["owner"], "ana");
            (
                200,
                json!({"url": "http://127.0.0.1:7002/google/start/c1#s", "redirect_uri": "http://127.0.0.1:7002/google/callback"}),
            )
        });
        let out = connect(&d, &http, "ana").unwrap();
        assert_eq!(out[1], "http://127.0.0.1:7002/google/start/c1#s");
        assert!(out[3].contains("/google/callback"));
        let none = FakeHttp::new(|_, _, _| (404, json!({})));
        assert!(
            connect(&d, &none, "zed")
                .unwrap_err()
                .to_string()
                .contains("no user named zed")
        );
        assert!(disconnect(&d, &none, "zed").is_err());
    }
}
