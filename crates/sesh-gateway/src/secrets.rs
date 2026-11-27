//! Seam: the only module that reaches the secret store (Infisical;
//! specs/004 research.md R5). Values pass through here to a connector's
//! upstream and nowhere else: never logged, returned or audited.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub enum SecretStore {
    Infisical {
        url: String,
        client_id: String,
        client_secret: String,
        project_id: String,
        environment: String,
        http: reqwest::Client,
        token: tokio::sync::Mutex<Option<(String, Instant)>>,
    },
    /// For tests: values in memory, or every call failing when `broken`.
    Memory {
        values: Mutex<BTreeMap<String, String>>,
        broken: bool,
    },
    /// No secret store configured: every credential is missing.
    None,
}

impl SecretStore {
    pub fn infisical(
        url: &str,
        client_id: &str,
        client_secret: &str,
        project_id: &str,
    ) -> Result<Self> {
        Ok(SecretStore::Infisical {
            url: url.trim_end_matches('/').to_string(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            project_id: project_id.into(),
            environment: "prod".into(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()?,
            token: tokio::sync::Mutex::new(None),
        })
    }

    pub fn memory() -> Self {
        SecretStore::Memory {
            values: Mutex::default(),
            broken: false,
        }
    }

    pub fn broken() -> Self {
        SecretStore::Memory {
            values: Mutex::default(),
            broken: true,
        }
    }

    /// A universal-auth access token, cached until shortly before expiry.
    async fn access_token(&self, fresh: bool) -> Result<String> {
        let SecretStore::Infisical {
            url,
            client_id,
            client_secret,
            http,
            token,
            ..
        } = self
        else {
            bail!("not Infisical");
        };
        let mut t = token.lock().await;
        if !fresh
            && let Some((tok, until)) = t.as_ref()
            && Instant::now() < *until
        {
            return Ok(tok.clone());
        }
        let r = http
            .post(format!("{url}/api/v1/auth/universal-auth/login"))
            .json(&json!({"clientId": client_id, "clientSecret": client_secret}))
            .send()
            .await
            .context("credential store unavailable")?;
        if !r.status().is_success() {
            bail!(
                "credential store refused the gateway's login ({})",
                r.status()
            );
        }
        let body: Value = r.json().await?;
        let tok = body["accessToken"]
            .as_str()
            .context("no access token")?
            .to_string();
        let secs = body["expiresIn"]
            .as_u64()
            .unwrap_or(600)
            .saturating_sub(60)
            .max(30);
        *t = Some((tok.clone(), Instant::now() + Duration::from_secs(secs)));
        Ok(tok)
    }

    /// A credential's value; `Ok(None)` when it isn't set.
    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        match self {
            SecretStore::None => Ok(None),
            SecretStore::Memory { values, broken } => {
                if *broken {
                    bail!("credential store unavailable");
                }
                Ok(values.lock().expect("lock").get(key).cloned())
            }
            SecretStore::Infisical {
                url,
                project_id,
                environment,
                http,
                ..
            } => {
                for fresh in [false, true] {
                    let tok = self.access_token(fresh).await?;
                    let r = http
                        .get(format!("{url}/api/v4/secrets/{key}"))
                        .bearer_auth(&tok)
                        .query(&[
                            ("projectId", project_id.as_str()),
                            ("environment", environment.as_str()),
                            ("secretPath", "/"),
                            ("viewSecretValue", "true"),
                            ("expandSecretReferences", "false"),
                        ])
                        .send()
                        .await
                        .context("credential store unavailable")?;
                    match r.status().as_u16() {
                        200 => {
                            let body: Value = r.json().await?;
                            return Ok(body["secret"]["secretValue"].as_str().map(str::to_string));
                        }
                        404 => return Ok(None),
                        401 if !fresh => continue,
                        s => bail!("credential store answered {s}"),
                    }
                }
                bail!("credential store refused the gateway")
            }
        }
    }

    /// Creates or replaces a credential.
    pub async fn put(&self, key: &str, value: &str) -> Result<()> {
        match self {
            SecretStore::None => bail!("no credential store configured"),
            SecretStore::Memory { values, broken } => {
                if *broken {
                    bail!("credential store unavailable");
                }
                values
                    .lock()
                    .expect("lock")
                    .insert(key.into(), value.into());
                Ok(())
            }
            SecretStore::Infisical {
                url,
                project_id,
                environment,
                http,
                ..
            } => {
                let tok = self.access_token(false).await?;
                let body = json!({"projectId": project_id, "environment": environment,
                    "secretPath": "/", "secretValue": value, "type": "shared"});
                let r = http
                    .patch(format!("{url}/api/v4/secrets/{key}"))
                    .bearer_auth(&tok)
                    .json(&body)
                    .send()
                    .await
                    .context("credential store unavailable")?;
                if r.status().is_success() {
                    return Ok(());
                }
                let r = http
                    .post(format!("{url}/api/v4/secrets/{key}"))
                    .bearer_auth(&tok)
                    .json(&body)
                    .send()
                    .await
                    .context("credential store unavailable")?;
                if !r.status().is_success() {
                    bail!(
                        "credential store refused to store the credential ({})",
                        r.status()
                    );
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_store_round_trips_and_fails_when_broken() {
        let s = SecretStore::memory();
        assert_eq!(s.get("TESTCRED__TOKEN").await.unwrap(), None);
        s.put("TESTCRED__TOKEN", "v1").await.unwrap();
        s.put("TESTCRED__TOKEN", "v2").await.unwrap();
        assert_eq!(
            s.get("TESTCRED__TOKEN").await.unwrap().as_deref(),
            Some("v2")
        );
        assert!(SecretStore::broken().get("X").await.is_err());
        assert_eq!(SecretStore::None.get("X").await.unwrap(), None);
    }

    /// A one-endpoint fake of Infisical's login and v4 secrets API.
    async fn fake_infisical() -> String {
        use axum::{
            Json, Router,
            extract::Path,
            http::StatusCode,
            routing::{get, post},
        };
        let store = std::sync::Arc::new(Mutex::new(BTreeMap::<String, String>::new()));
        let (s1, s2, s3) = (store.clone(), store.clone(), store.clone());
        let app = Router::new()
            .route(
                "/api/v1/auth/universal-auth/login",
                post(|Json(b): Json<Value>| async move {
                    if b["clientSecret"] == "good" {
                        (
                            StatusCode::OK,
                            Json(json!({"accessToken": "at", "expiresIn": 600})),
                        )
                    } else {
                        (StatusCode::UNAUTHORIZED, Json(json!({})))
                    }
                }),
            )
            .route(
                "/api/v4/secrets/{key}",
                get(move |Path(k): Path<String>| async move {
                    match s1.lock().unwrap().get(&k) {
                        Some(v) => (StatusCode::OK, Json(json!({"secret": {"secretValue": v}}))),
                        None => (StatusCode::NOT_FOUND, Json(json!({}))),
                    }
                })
                .patch(
                    move |Path(k): Path<String>, Json(b): Json<Value>| async move {
                        let mut m = s2.lock().unwrap();
                        match m.get_mut(&k) {
                            Some(v) => {
                                *v = b["secretValue"].as_str().unwrap().into();
                                StatusCode::OK
                            }
                            None => StatusCode::NOT_FOUND,
                        }
                    },
                )
                .post(
                    move |Path(k): Path<String>, Json(b): Json<Value>| async move {
                        s3.lock()
                            .unwrap()
                            .insert(k, b["secretValue"].as_str().unwrap().into());
                        StatusCode::OK
                    },
                ),
            );
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(l, app).await });
        url
    }

    #[tokio::test]
    async fn infisical_client_creates_replaces_and_reads() {
        let url = fake_infisical().await;
        let s = SecretStore::infisical(&url, "id", "good", "p1").unwrap();
        assert_eq!(s.get("TESTCRED__TOKEN").await.unwrap(), None);
        s.put("TESTCRED__TOKEN", "first").await.unwrap();
        s.put("TESTCRED__TOKEN", "second").await.unwrap();
        assert_eq!(
            s.get("TESTCRED__TOKEN").await.unwrap().as_deref(),
            Some("second")
        );
        let bad = SecretStore::infisical(&url, "id", "bad", "p1").unwrap();
        assert!(bad.get("TESTCRED__TOKEN").await.is_err());
        let down = SecretStore::infisical("http://127.0.0.1:9", "id", "good", "p1").unwrap();
        let e = down.get("X").await.unwrap_err().to_string();
        assert!(e.contains("credential store unavailable"), "{e}");
    }
}
