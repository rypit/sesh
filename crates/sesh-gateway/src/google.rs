//! Seam: the only module that reaches Google (specs/005 research.md R3,
//! ADR 0008). It runs the consent flow (one-time link, single-use state,
//! PKCE), keeps each owner's refresh token in the secret store, and turns
//! it into short-lived access tokens held only in memory. No token ever
//! leaves this module except in requests to Google.

use crate::approvals::{ct_eq, new_id, new_secret, sha256_hex};
use crate::secrets::SecretStore;
use anyhow::{Context, Result, bail};
use base64::Engine;
use serde_json::{Value, json};
use sesh_core::Upstreams;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The Google access sesh asks for (FR-007): nothing broader.
pub const SCOPES: [&str; 4] = [
    "https://www.googleapis.com/auth/gmail.readonly",
    "https://www.googleapis.com/auth/gmail.compose",
    "https://www.googleapis.com/auth/calendar.events",
    "https://www.googleapis.com/auth/contacts.readonly",
];

const LINK_TTL: Duration = Duration::from_secs(1800);
const STATE_TTL: Duration = Duration::from_secs(600);

/// Why a Google call couldn't be made, in words an agent can act on.
#[derive(Debug, Clone, PartialEq)]
pub enum GErr {
    NotConnected,
    Revoked,
    NotPermitted,
    Unavailable(String),
    Api(u16, String),
}

impl GErr {
    pub fn message(&self) -> String {
        match self {
            GErr::NotConnected => "Google isn't connected for this user".into(),
            GErr::Revoked => "Google access was revoked; reconnect".into(),
            GErr::NotPermitted => "not permitted by this Google connection".into(),
            GErr::Unavailable(why) => format!("Google is unavailable ({why})"),
            GErr::Api(s, why) => format!("Google answered {s}: {why}"),
        }
    }
}

fn key(owner: &str, what: &str) -> String {
    format!(
        "USER_{}__GOOGLE__{what}",
        owner.to_ascii_uppercase().replace('-', "_")
    )
}

pub fn refresh_key(owner: &str) -> String {
    key(owner, "REFRESH_TOKEN")
}
pub fn scopes_key(owner: &str) -> String {
    key(owner, "SCOPES")
}
pub fn state_key(owner: &str) -> String {
    key(owner, "STATE")
}

struct Consent {
    owner: String,
    secret_sha256: String,
    link_until: Instant,
    used: bool,
    /// Set when the start page is used.
    state_sha256: Option<String>,
    verifier: Option<String>,
    state_until: Option<Instant>,
}

pub struct Google {
    http: reqwest::Client,
    consents: Mutex<HashMap<String, Consent>>,
    tokens: tokio::sync::Mutex<HashMap<String, (String, Instant)>>,
}

impl Default for Google {
    fn default() -> Self {
        Google {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("http client"),
            consents: Mutex::default(),
            tokens: Default::default(),
        }
    }
}

fn b64url(b: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

/// The PKCE S256 challenge for a verifier.
pub fn challenge(verifier: &str) -> String {
    b64url(&Sha256::digest(verifier.as_bytes()))
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

async fn client_creds(secrets: &SecretStore) -> Result<(String, String), GErr> {
    let get = |n: &'static str| async move {
        secrets
            .get(&sesh_core::credential_key("google", n))
            .await
            .map_err(|e| GErr::Unavailable(e.to_string()))
    };
    match (get("CLIENT_ID").await?, get("CLIENT_SECRET").await?) {
        (Some(id), Some(sec)) => Ok((id, sec)),
        _ => Err(GErr::Unavailable("the Google OAuth client isn't set up (sesh credential request google CLIENT_ID / CLIENT_SECRET)".into())),
    }
}

impl Google {
    /// A one-time consent link for `owner`: `(id, secret)`; the secret goes
    /// in the link's fragment.
    pub fn new_consent(&self, owner: &str) -> (String, String) {
        let (id, secret) = (new_id(), new_secret());
        let mut c = self.consents.lock().expect("lock");
        let now = Instant::now();
        c.retain(|_, x| x.link_until > now || x.state_until.is_some_and(|u| u > now));
        c.insert(
            id.clone(),
            Consent {
                owner: owner.into(),
                secret_sha256: sha256_hex(&secret),
                link_until: now + LINK_TTL,
                used: false,
                state_sha256: None,
                verifier: None,
                state_until: None,
            },
        );
        (id, secret)
    }

    /// Uses the link once; returns Google's authorization URL to send the
    /// person to.
    pub async fn start(
        &self,
        up: &Upstreams,
        secrets: &SecretStore,
        id: &str,
        secret: &str,
        redirect_uri: &str,
    ) -> Result<(String, String), String> {
        let (client_id, _) = client_creds(secrets).await.map_err(|e| e.message())?;
        let verifier = new_secret() + &new_secret();
        let state = new_secret();
        let owner = {
            let mut c = self.consents.lock().expect("lock");
            let Some(x) = c.get_mut(id) else {
                return Err("This link isn't valid.".into());
            };
            if !ct_eq(&sha256_hex(secret), &x.secret_sha256) {
                return Err("This link isn't valid.".into());
            }
            if x.used {
                return Err("This link was already used.".into());
            }
            if x.link_until <= Instant::now() {
                return Err("This link expired.".into());
            }
            x.used = true;
            x.state_sha256 = Some(sha256_hex(&state));
            x.verifier = Some(verifier.clone());
            x.state_until = Some(Instant::now() + STATE_TTL);
            x.owner.clone()
        };
        let url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&include_granted_scopes=false&code_challenge={}&code_challenge_method=S256&state={}",
            up.google_auth,
            enc(&client_id),
            enc(redirect_uri),
            enc(&SCOPES.join(" ")),
            challenge(&verifier),
            enc(&format!("{id}.{state}"))
        );
        Ok((url, owner))
    }

    /// Finishes consent: checks the state, exchanges the code, and stores
    /// the grant under the owner. Returns `(owner, granted scopes)`.
    pub async fn finish(
        &self,
        up: &Upstreams,
        secrets: &SecretStore,
        state: &str,
        code: &str,
        redirect_uri: &str,
    ) -> Result<(String, Vec<String>), String> {
        let (id, st) = state.split_once('.').ok_or("This sign-in isn't valid.")?;
        let (owner, verifier) = {
            let mut c = self.consents.lock().expect("lock");
            let Some(x) = c.remove(id) else {
                return Err("This sign-in isn't valid or was already used.".into());
            };
            let ok = x
                .state_sha256
                .as_deref()
                .is_some_and(|h| ct_eq(&sha256_hex(st), h))
                && x.state_until.is_some_and(|u| u > Instant::now());
            if !ok {
                return Err("This sign-in isn't valid or expired.".into());
            }
            (x.owner, x.verifier.unwrap_or_default())
        };
        let (client_id, client_secret) = client_creds(secrets).await.map_err(|e| e.message())?;
        let r = self
            .http
            .post(&up.google_token)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect_uri),
                ("client_id", &client_id),
                ("client_secret", &client_secret),
                ("code_verifier", &verifier),
            ])
            .send()
            .await
            .map_err(|_| "Google is unreachable.".to_string())?;
        let status = r.status();
        let body: Value = r.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(format!("Google refused the sign-in ({status})."));
        }
        let refresh = body["refresh_token"]
            .as_str()
            .ok_or("Google returned no long-lived grant; try again.")?;
        let scopes: Vec<String> = body["scope"]
            .as_str()
            .unwrap_or("")
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let store = async {
            secrets.put(&refresh_key(&owner), refresh).await?;
            secrets.put(&scopes_key(&owner), &scopes.join(" ")).await?;
            secrets.put(&state_key(&owner), "connected").await
        };
        store
            .await
            .map_err(|_| "The secret store is unavailable; try again.".to_string())?;
        self.tokens.lock().await.remove(&owner);
        Ok((owner, scopes))
    }

    /// Revokes and forgets the owner's grant.
    pub async fn disconnect(
        &self,
        up: &Upstreams,
        secrets: &SecretStore,
        owner: &str,
    ) -> Result<()> {
        if let Some(rt) = secrets.get(&refresh_key(owner)).await? {
            let _ = self
                .http
                .post(&up.google_revoke)
                .form(&[("token", rt.as_str())])
                .send()
                .await;
        }
        secrets.delete(&refresh_key(owner)).await?;
        secrets.delete(&scopes_key(owner)).await?;
        secrets.put(&state_key(owner), "removed").await?;
        self.tokens.lock().await.remove(owner);
        Ok(())
    }

    /// The owner's connection state: connected, revoked, or missing.
    pub async fn status(&self, secrets: &SecretStore, owner: &str) -> Result<String> {
        let has = secrets.get(&refresh_key(owner)).await?.is_some();
        let state = secrets.get(&state_key(owner)).await?.unwrap_or_default();
        Ok(match (has, state.as_str()) {
            (true, _) => "connected".into(),
            (false, "revoked") => "revoked".into(),
            _ => "missing".into(),
        })
    }

    /// Whether the owner's grant includes `scope`.
    pub async fn permits(
        &self,
        secrets: &SecretStore,
        owner: &str,
        scope: &str,
    ) -> Result<bool, GErr> {
        let s = secrets
            .get(&scopes_key(owner))
            .await
            .map_err(|e| GErr::Unavailable(e.to_string()))?
            .unwrap_or_default();
        Ok(s.split_whitespace().any(|x| x == scope))
    }

    async fn access_token(
        &self,
        up: &Upstreams,
        secrets: &SecretStore,
        owner: &str,
    ) -> Result<String, GErr> {
        if let Some((t, until)) = self.tokens.lock().await.get(owner)
            && Instant::now() < *until
        {
            return Ok(t.clone());
        }
        let Some(rt) = secrets
            .get(&refresh_key(owner))
            .await
            .map_err(|e| GErr::Unavailable(e.to_string()))?
        else {
            let st = secrets
                .get(&state_key(owner))
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
            return Err(if st == "revoked" {
                GErr::Revoked
            } else {
                GErr::NotConnected
            });
        };
        let (id, sec) = client_creds(secrets).await?;
        let r = self
            .http
            .post(&up.google_token)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", rt.as_str()),
                ("client_id", id.as_str()),
                ("client_secret", sec.as_str()),
            ])
            .send()
            .await
            .map_err(|_| GErr::Unavailable("unreachable".into()))?;
        let status = r.status();
        let body: Value = r.json().await.unwrap_or(Value::Null);
        if body["error"] == "invalid_grant" {
            // Revoked from Google's side: forget the grant (spec edge case).
            let _ = secrets.delete(&refresh_key(owner)).await;
            let _ = secrets.put(&state_key(owner), "revoked").await;
            return Err(GErr::Revoked);
        }
        if !status.is_success() {
            return Err(GErr::Unavailable(format!(
                "token endpoint answered {status}"
            )));
        }
        let tok = body["access_token"]
            .as_str()
            .ok_or(GErr::Unavailable("no access token".into()))?
            .to_string();
        let secs = body["expires_in"]
            .as_u64()
            .unwrap_or(3600)
            .saturating_sub(60)
            .max(30);
        self.tokens.lock().await.insert(
            owner.into(),
            (tok.clone(), Instant::now() + Duration::from_secs(secs)),
        );
        Ok(tok)
    }

    /// One Google API call as the owner. `body` is JSON for POST, or None for GET.
    pub async fn api(
        &self,
        up: &Upstreams,
        secrets: &SecretStore,
        owner: &str,
        method: &str,
        url: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<Value, GErr> {
        let tok = self.access_token(up, secrets, owner).await?;
        let mut req = match method {
            "POST" => self.http.post(url),
            _ => self.http.get(url),
        }
        .bearer_auth(&tok)
        .query(query);
        if let Some(b) = body {
            req = req.json(b);
        }
        let r = req
            .send()
            .await
            .map_err(|_| GErr::Unavailable("unreachable".into()))?;
        let status = r.status().as_u16();
        let v: Value = r.json().await.unwrap_or(Value::Null);
        match status {
            200..=299 => Ok(v),
            401 => {
                self.tokens.lock().await.remove(owner);
                Err(GErr::Api(401, "unauthorized".into()))
            }
            403 => Err(GErr::NotPermitted),
            s => Err(GErr::Api(
                s,
                v["error"]["message"]
                    .as_str()
                    .unwrap_or("error")
                    .chars()
                    .take(200)
                    .collect(),
            )),
        }
    }
}

/// Builds `raw` for Gmail: an RFC 822 message, base64url. Header values
/// must have no line breaks (checked by the caller's validation).
pub fn raw_message(to: &[String], cc: &[String], subject: &str, body: &str) -> Result<String> {
    for h in to
        .iter()
        .chain(cc)
        .chain(std::iter::once(&subject.to_string()))
    {
        if h.contains('\r') || h.contains('\n') {
            bail!("header values can't contain line breaks");
        }
    }
    let mut m = format!("To: {}\r\n", to.join(", "));
    if !cc.is_empty() {
        m.push_str(&format!("Cc: {}\r\n", cc.join(", ")));
    }
    let subj = if subject.is_ascii() {
        subject.to_string()
    } else {
        format!(
            "=?UTF-8?B?{}?=",
            base64::engine::general_purpose::STANDARD.encode(subject)
        )
    };
    m.push_str(&format!("Subject: {subj}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}"));
    Ok(b64url(m.as_bytes()))
}

/// Decodes Gmail's base64url body data.
pub fn decode(data: &str) -> Result<Vec<u8>> {
    let d = data.replace('-', "+").replace('_', "/");
    let d = d.trim_end_matches('=');
    base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(d)
        .context("bad base64 from Google")
}

pub fn json_ok(v: Value) -> Value {
    json!({"ok": true, "result": v})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_matches_rfc7636() {
        // RFC 7636 appendix B.
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn raw_messages_refuse_header_injection() {
        let ok = raw_message(&["a@x.org".into()], &[], "Hi", "Body").unwrap();
        let text = String::from_utf8(decode(&ok).unwrap()).unwrap();
        assert!(text.starts_with("To: a@x.org\r\nSubject: Hi\r\n"));
        assert!(raw_message(&["a@x.org\r\nBcc: evil@x.org".into()], &[], "Hi", "b").is_err());
        assert!(raw_message(&["a@x.org".into()], &[], "Hi\nBcc: e@x.org", "b").is_err());
    }

    #[test]
    fn keys_are_per_owner() {
        assert_eq!(refresh_key("ana"), "USER_ANA__GOOGLE__REFRESH_TOKEN");
        assert_eq!(
            refresh_key("household"),
            "USER_HOUSEHOLD__GOOGLE__REFRESH_TOKEN"
        );
        assert_eq!(scopes_key("ben-2"), "USER_BEN_2__GOOGLE__SCOPES");
    }

    /// A fake Google: authorization is skipped; token, revoke and one API.
    async fn fake() -> String {
        use axum::{
            Form, Json, Router,
            http::StatusCode,
            routing::{get, post},
        };
        let app = Router::new()
            .route("/token", post(|Form(f): Form<HashMap<String, String>>| async move {
                match f.get("grant_type").map(String::as_str) {
                    Some("authorization_code") if f.get("code_verifier").is_some_and(|v| !v.is_empty()) => (
                        StatusCode::OK,
                        Json(json!({"access_token": "at-1", "refresh_token": "rt-ana", "expires_in": 3600, "scope": SCOPES[..2].join(" ")})),
                    ),
                    Some("refresh_token") if f.get("refresh_token").map(String::as_str) == Some("rt-ana") => {
                        (StatusCode::OK, Json(json!({"access_token": "at-2", "expires_in": 3600})))
                    }
                    Some("refresh_token") => (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid_grant"}))),
                    _ => (StatusCode::BAD_REQUEST, Json(json!({"error": "invalid_request"}))),
                }
            }))
            .route("/revoke", post(|| async { StatusCode::OK }))
            .route("/api", get(|h: axum::http::HeaderMap| async move {
                let ok = h.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer at-2");
                if ok { (StatusCode::OK, Json(json!({"hello": "ana"}))) } else { (StatusCode::UNAUTHORIZED, Json(json!({}))) }
            }));
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(l, app).await });
        url
    }

    fn ups(base: &str) -> Upstreams {
        Upstreams {
            google_auth: format!("{base}/auth"),
            google_token: format!("{base}/token"),
            google_revoke: format!("{base}/revoke"),
            ..Upstreams::default()
        }
    }

    #[tokio::test]
    async fn consent_stores_the_grant_once_and_calls_work() {
        let base = fake().await;
        let up = ups(&base);
        let s = SecretStore::memory();
        s.put("GOOGLE__CLIENT_ID", "cid").await.unwrap();
        s.put("GOOGLE__CLIENT_SECRET", "csec").await.unwrap();
        let g = Google::default();
        assert_eq!(g.status(&s, "ana").await.unwrap(), "missing");
        assert_eq!(
            g.api(&up, &s, "ana", "GET", &format!("{base}/api"), &[], None)
                .await
                .unwrap_err(),
            GErr::NotConnected
        );
        let (id, secret) = g.new_consent("ana");
        assert!(g.start(&up, &s, &id, "wrong", "http://cb").await.is_err());
        let (url, owner) = g.start(&up, &s, &id, &secret, "http://cb").await.unwrap();
        assert_eq!(owner, "ana");
        assert!(url.contains("code_challenge_method=S256") && url.contains("access_type=offline"));
        assert!(
            !url.contains("csec"),
            "the client secret never goes in the browser URL"
        );
        assert!(
            g.start(&up, &s, &id, &secret, "http://cb").await.is_err(),
            "the link works once"
        );
        let state = url.split("state=").nth(1).unwrap().replace("%2E", ".");
        assert!(
            g.finish(&up, &s, "x.y", "code", "http://cb").await.is_err(),
            "a forged state"
        );
        let (who, scopes) = g
            .finish(&up, &s, &state, "code", "http://cb")
            .await
            .unwrap();
        assert_eq!((who.as_str(), scopes.len()), ("ana", 2));
        assert!(
            g.finish(&up, &s, &state, "code", "http://cb")
                .await
                .is_err(),
            "the state works once"
        );
        assert_eq!(g.status(&s, "ana").await.unwrap(), "connected");
        assert!(g.permits(&s, "ana", SCOPES[0]).await.unwrap());
        assert!(
            !g.permits(&s, "ana", SCOPES[2]).await.unwrap(),
            "a scope not granted"
        );
        assert_eq!(
            g.api(&up, &s, "ana", "GET", &format!("{base}/api"), &[], None)
                .await
                .unwrap()["hello"],
            "ana"
        );
        assert_eq!(g.status(&s, "ben").await.unwrap(), "missing", "per owner");
        g.disconnect(&up, &s, "ana").await.unwrap();
        assert_eq!(g.status(&s, "ana").await.unwrap(), "missing");
        assert_eq!(
            g.api(&up, &s, "ana", "GET", &format!("{base}/api"), &[], None)
                .await
                .unwrap_err(),
            GErr::NotConnected
        );
    }

    #[tokio::test]
    async fn a_revoked_grant_is_forgotten() {
        let base = fake().await;
        let up = ups(&base);
        let s = SecretStore::memory();
        s.put("GOOGLE__CLIENT_ID", "cid").await.unwrap();
        s.put("GOOGLE__CLIENT_SECRET", "csec").await.unwrap();
        s.put(&refresh_key("ben"), "rt-revoked").await.unwrap();
        let g = Google::default();
        assert_eq!(
            g.api(&up, &s, "ben", "GET", &format!("{base}/api"), &[], None)
                .await
                .unwrap_err(),
            GErr::Revoked
        );
        assert_eq!(g.status(&s, "ben").await.unwrap(), "revoked");
        assert!(s.get(&refresh_key("ben")).await.unwrap().is_none());
    }
}
