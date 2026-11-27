//! Seam: the only module that reaches Paperless-ngx (specs/005 research.md
//! R6, ADR 0008). It acts as Paperless's admin, and enforces owners
//! itself: every search and read is filtered to the caller's readable
//! owners, and every upload is given the caller's owner. Attachment bytes
//! arrive from the mail connector by reference and never leave the gateway.

use crate::Gateway;
use crate::owners::Scope;
use serde_json::{Value, json};
use sesh_core::{ConnectorDescriptor, HOUSEHOLD};
use std::collections::HashMap;
use std::time::Duration;

const CONTENT_CAP: usize = 20_000;

/// PDF and images only: Office files need Tika and Gotenberg, which sesh
/// doesn't run (specs/005 research.md R6).
pub const UPLOAD_TYPES: [&str; 4] = ["application/pdf", "image/png", "image/jpeg", "image/tiff"];

pub fn descriptor() -> ConnectorDescriptor {
    let s = |min: u64, max: u64| json!({"type": "string", "minLength": min, "maxLength": max});
    super::connector(
        "docs",
        "Your owner's documents (and the Household's, read-only, for a personal agent). File email attachments by their reference; never paste file contents.",
        vec![
            (
                "search",
                super::method(
                    "Full-text search of documents (up to 25).",
                    json!({"type": "object", "required": ["query"], "properties": {"query": s(1, 300), "limit": json!({"type": "integer", "minimum": 1, "maximum": 25})}}),
                    false,
                ),
            ),
            (
                "get",
                super::method(
                    "A document's title, tags and text.",
                    json!({"type": "object", "required": ["id"], "properties": {"id": json!({"type": "integer", "minimum": 1, "maximum": 1_000_000_000})}}),
                    false,
                ),
            ),
            (
                "upload",
                super::method(
                    "File an email attachment, by the reference mail.read gave you, into your owner's documents.",
                    json!({"type": "object", "required": ["ref", "title"], "properties": {"ref": s(32, 32), "title": s(1, 200),
                    "tags": json!({"type": "array", "items": s(1, 50), "maxItems": 10})}}),
                    false,
                ),
            ),
        ],
    )
}

#[derive(Clone)]
pub struct Paperless {
    http: reqwest::Client,
    user: String,
    password: String,
    token: std::sync::Arc<tokio::sync::Mutex<Option<String>>>,
    ids: std::sync::Arc<tokio::sync::Mutex<HashMap<String, u64>>>,
}

/// How long an upload waits inline for Paperless to consume the file;
/// past it, a background task finishes (an execution's deadline is 30 s).
const INLINE_POLLS: u32 = 40;
const BACKGROUND_POLLS: u32 = 600;

impl Paperless {
    pub fn new(user: &str, password: &str) -> Self {
        Paperless {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("http client"),
            user: user.into(),
            password: password.into(),
            token: Default::default(),
            ids: Default::default(),
        }
    }

    async fn token(&self, base: &str) -> Result<String, String> {
        let mut t = self.token.lock().await;
        if let Some(x) = t.as_ref() {
            return Ok(x.clone());
        }
        if self.user.is_empty() {
            return Err("the document service isn't configured".into());
        }
        let r = self
            .http
            .post(format!("{base}/api/token/"))
            .form(&[
                ("username", self.user.as_str()),
                ("password", self.password.as_str()),
            ])
            .send()
            .await
            .map_err(|_| "the document service is unavailable".to_string())?;
        let v: Value = r
            .json()
            .await
            .map_err(|_| "the document service sent an unreadable reply".to_string())?;
        let tok = v["token"]
            .as_str()
            .ok_or("the document service refused the gateway")?
            .to_string();
        *t = Some(tok.clone());
        Ok(tok)
    }

    async fn req(
        &self,
        base: &str,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Value, String> {
        let tok = self.token(base).await?;
        let mut r = self
            .http
            .request(method, format!("{base}{path}"))
            .header("Authorization", format!("Token {tok}"))
            .query(query);
        if let Some(b) = body {
            r = r.json(&b);
        }
        let resp = r
            .send()
            .await
            .map_err(|_| "the document service is unavailable".to_string())?;
        let status = resp.status().as_u16();
        if status == 401 {
            *self.token.lock().await = None;
        }
        if status == 404 {
            return Err("document not found".into());
        }
        if !(200..300).contains(&status) {
            return Err(format!("the document service answered {status}"));
        }
        resp.json()
            .await
            .map_err(|_| "the document service sent an unreadable reply".to_string())
    }

    /// Doctor: the gateway can sign in and list documents.
    pub async fn check(&self, base: &str) -> Result<(), String> {
        self.req(
            base,
            reqwest::Method::GET,
            "/api/documents/",
            &[("page_size", "1".into())],
            None,
        )
        .await
        .map(|_| ())
    }

    /// The Paperless user id for an owner, created the first time.
    pub async fn owner_id(&self, base: &str, owner: &str) -> Result<u64, String> {
        if let Some(id) = self.ids.lock().await.get(owner) {
            return Ok(*id);
        }
        let name = format!("sesh-{owner}");
        let found = self
            .req(
                base,
                reqwest::Method::GET,
                "/api/users/",
                &[("username__iexact", name.clone())],
                None,
            )
            .await?;
        let id = match found["results"][0]["id"].as_u64() {
            Some(id) => id,
            None => {
                // A Paperless account no one logs in to: a long random password.
                let pw = crate::approvals::new_secret();
                let made = self
                    .req(
                        base,
                        reqwest::Method::POST,
                        "/api/users/",
                        &[],
                        Some(json!({"username": name, "password": pw, "is_active": true})),
                    )
                    .await?;
                made["id"]
                    .as_u64()
                    .ok_or("the document service didn't create the user")?
            }
        };
        self.ids.lock().await.insert(owner.into(), id);
        Ok(id)
    }

    async fn tag_ids(
        &self,
        base: &str,
        owner_id: u64,
        tags: &[String],
    ) -> Result<Vec<u64>, String> {
        let mut ids = vec![];
        for t in tags {
            let found = self
                .req(
                    base,
                    reqwest::Method::GET,
                    "/api/tags/",
                    &[("name__iexact", t.clone())],
                    None,
                )
                .await?;
            let id = match found["results"][0]["id"].as_u64() {
                Some(id) => id,
                None => self
                    .req(
                        base,
                        reqwest::Method::POST,
                        "/api/tags/",
                        &[],
                        Some(json!({"name": t, "owner": owner_id})),
                    )
                    .await?["id"]
                    .as_u64()
                    .ok_or("couldn't create the tag")?,
            };
            ids.push(id);
        }
        Ok(ids)
    }

    /// Hands the file to Paperless; returns its consumption task id.
    async fn upload(
        &self,
        base: &str,
        filename: &str,
        mime: &str,
        bytes: Vec<u8>,
        title: &str,
        tags: &[u64],
    ) -> Result<String, String> {
        let tok = self.token(base).await?;
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(filename.to_string())
            .mime_str(mime)
            .map_err(|_| "file type not allowed".to_string())?;
        let mut form = reqwest::multipart::Form::new()
            .part("document", part)
            .text("title", title.to_string());
        for t in tags {
            form = form.text("tags", t.to_string());
        }
        let r = self
            .http
            .post(format!("{base}/api/documents/post_document/"))
            .header("Authorization", format!("Token {tok}"))
            .multipart(form)
            .send()
            .await
            .map_err(|_| "the document service is unavailable".to_string())?;
        if !r.status().is_success() {
            return Err(format!(
                "the document service refused the file ({})",
                r.status()
            ));
        }
        let task: Value = r
            .json()
            .await
            .map_err(|_| "the document service sent an unreadable reply".to_string())?;
        let task = task
            .as_str()
            .map(str::to_string)
            .or(task["task_id"].as_str().map(str::to_string))
            .ok_or("no task id")?;
        Ok(task)
    }

    /// Waits for consumption, then gives the document its owner. Until
    /// then it belongs to the gateway's admin account, which no agent's
    /// scope includes, so it is invisible rather than misfiled.
    async fn finish(
        &self,
        base: &str,
        task: &str,
        owner_id: u64,
        polls: u32,
    ) -> Result<Option<u64>, String> {
        let task = task.to_string();
        for _ in 0..polls {
            let t = self
                .req(
                    base,
                    reqwest::Method::GET,
                    "/api/tasks/",
                    &[("task_id", task.clone())],
                    None,
                )
                .await?;
            let t = if t.is_array() {
                t[0].clone()
            } else {
                t["results"][0].clone()
            };
            match t["status"].as_str() {
                Some("SUCCESS") => {
                    let id = t["related_document"]
                        .as_u64()
                        .or(t["related_document"].as_str().and_then(|s| s.parse().ok()))
                        .ok_or("the document service gave no document id")?;
                    self.req(
                        base,
                        reqwest::Method::PATCH,
                        &format!("/api/documents/{id}/"),
                        &[],
                        Some(json!({"owner": owner_id})),
                    )
                    .await?;
                    return Ok(Some(id));
                }
                Some("FAILURE") => {
                    return Err(format!(
                        "the document service couldn't process the file: {}",
                        t["result"]
                            .as_str()
                            .unwrap_or("")
                            .chars()
                            .take(200)
                            .collect::<String>()
                    ));
                }
                _ => tokio::time::sleep(Duration::from_millis(500)).await,
            }
        }
        Ok(None)
    }
}

fn cut(s: &str, n: usize) -> String {
    if s.len() <= n {
        return s.to_string();
    }
    let mut end = n;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… [cut]", &s[..end])
}

pub async fn call(
    gw: &Gateway,
    scope: &Scope,
    agent: &str,
    method: &str,
    args: &Value,
) -> Result<Value, String> {
    let base = gw
        .settings
        .upstreams
        .paperless
        .trim_end_matches('/')
        .to_string();
    let p = &gw.paperless;
    let mut readable = HashMap::new();
    for o in &scope.readable {
        readable.insert(p.owner_id(&base, o).await?, o.clone());
    }
    let label = |owner: &str| {
        if owner == scope.owner {
            "you"
        } else {
            HOUSEHOLD
        }
    };
    match method {
        "search" => {
            let ids: Vec<String> = readable.keys().map(|i| i.to_string()).collect();
            let v = p
                .req(
                    &base,
                    reqwest::Method::GET,
                    "/api/documents/",
                    &[
                        ("query", args["query"].as_str().unwrap_or("").into()),
                        ("owner__id__in", ids.join(",")),
                        (
                            "page_size",
                            args["limit"].as_u64().unwrap_or(10).to_string(),
                        ),
                    ],
                    None,
                )
                .await?;
            let out: Vec<Value> = v["results"]
                .as_array()
                .into_iter()
                .flatten()
                // Filter again here: the gateway, not Paperless, enforces owners.
                .filter_map(|d| {
                    let o = readable.get(&d["owner"].as_u64()?)?;
                    Some(json!({"id": d["id"], "title": d["title"], "created": d["created"], "tags": d["tags"], "owner": label(o)}))
                })
                .collect();
            Ok(json!(out))
        }
        "get" => {
            let id = args["id"].as_u64().unwrap_or(0);
            let d = p
                .req(
                    &base,
                    reqwest::Method::GET,
                    &format!("/api/documents/{id}/"),
                    &[],
                    None,
                )
                .await?;
            if d["owner"]
                .as_u64()
                .is_none_or(|o| !readable.contains_key(&o))
            {
                return Err("document not found".into());
            }
            Ok(
                json!({"id": d["id"], "title": d["title"], "tags": d["tags"], "created": d["created"], "content": cut(d["content"].as_str().unwrap_or(""), CONTENT_CAP)}),
            )
        }
        "upload" => {
            let Some(att) = gw.refs.resolve(args["ref"].as_str().unwrap_or(""), agent) else {
                return Err("unknown or expired attachment reference".into());
            };
            if !UPLOAD_TYPES.contains(&att.mime.as_str()) {
                return Err("file type not allowed".into());
            }
            if att.size > gw.settings.upload_max_bytes {
                return Err("file too large".into());
            }
            let bytes = super::google::attachment_bytes(gw, &att).await?;
            if bytes.len() as u64 > gw.settings.upload_max_bytes {
                return Err("file too large".into());
            }
            let owner_id = p.owner_id(&base, &scope.owner).await?;
            let tags: Vec<String> = args["tags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect();
            let tag_ids = p.tag_ids(&base, owner_id, &tags).await?;
            let title = args["title"].as_str().unwrap_or("").to_string();
            let task = p
                .upload(&base, &att.filename, &att.mime, bytes, &title, &tag_ids)
                .await?;
            if scope.owner == HOUSEHOLD {
                // FR-014: a household document tells everyone.
                let _ = gw
                    .notifier
                    .send(&crate::notify::Note {
                        topic: scope.topic.clone(),
                        title: "New household document".into(),
                        message: format!(
                            "\"{}\" was filed to the household's documents.",
                            title.chars().take(200).collect::<String>()
                        ),
                        link: String::new(),
                    })
                    .await;
            }
            match p.finish(&base, &task, owner_id, INLINE_POLLS).await? {
                Some(id) => Ok(json!({"id": id, "title": title, "status": "filed"})),
                None => {
                    let (p, base) = (p.clone(), base.clone());
                    tokio::spawn(async move {
                        if let Err(e) = p.finish(&base, &task, owner_id, BACKGROUND_POLLS).await {
                            tracing::warn!("filing a document failed: {e}");
                        }
                    });
                    Ok(
                        json!({"title": title, "status": "processing", "note": "the document service is still reading the file; it will be searchable shortly"}),
                    )
                }
            }
        }
        _ => Err(format!("docs.{method} has no upstream")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Audit;
    use crate::calls::tests::gateway;
    use crate::refs::Attachment;
    use axum::extract::{Path, Query};
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use sesh_core::GatewaySettings;

    /// A Paperless that, unhelpfully, returns everyone's documents: the
    /// gateway must filter them itself.
    async fn fake_paperless() -> String {
        let docs = || {
            vec![
                json!({"id": 10, "owner": 1, "title": "household lease", "content": "lease", "tags": [], "created": "2026-01-01"}),
                json!({"id": 11, "owner": 2, "title": "ana payslip", "content": "pay", "tags": [], "created": "2026-01-01"}),
                json!({"id": 12, "owner": 3, "title": "ben medical", "content": "private", "tags": [], "created": "2026-01-01"}),
            ]
        };
        let app = Router::new()
            .route(
                "/api/token/",
                post(|| async { Json(json!({"token": "ptok"})) }),
            )
            .route(
                "/api/users/",
                get(|Query(q): Query<HashMap<String, String>>| async move {
                    let id = match q.get("username__iexact").map(String::as_str) {
                        Some("sesh-household") => 1,
                        Some("sesh-ana") => 2,
                        Some("sesh-ben") => 3,
                        _ => return Json(json!({"results": []})),
                    };
                    Json(json!({"results": [{"id": id}]}))
                }),
            )
            .route(
                "/api/documents/",
                get(move || async move { Json(json!({"results": docs()})) }),
            )
            .route(
                "/api/documents/{id}/",
                get(move |Path(id): Path<u64>| async move {
                    Json(
                        docs()
                            .into_iter()
                            .find(|d| d["id"] == id)
                            .unwrap_or(json!({})),
                    )
                }),
            );
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(l, app).await });
        url
    }

    async fn gw() -> Gateway {
        let mut s = GatewaySettings::default();
        s.upstreams.paperless = fake_paperless().await;
        let mut g = gateway(Audit::memory(), s);
        g.paperless = Paperless::new("sesh-gateway", "pw");
        g
    }

    fn ana() -> Scope {
        Scope {
            owner: "ana".into(),
            topic: "t".into(),
            readable: vec!["ana".into(), HOUSEHOLD.into()],
        }
    }

    #[tokio::test]
    async fn a_user_sees_their_own_and_the_household_documents_only() {
        let g = gw().await;
        let found = call(&g, &ana(), "alice", "search", &json!({"query": "x"}))
            .await
            .unwrap();
        let ids: Vec<u64> = found
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["id"].as_u64().unwrap())
            .collect();
        assert_eq!(ids, vec![10, 11]);
        assert_eq!(found[0]["owner"], "household");
        assert_eq!(found[1]["owner"], "you");
        assert!(
            call(&g, &ana(), "alice", "get", &json!({"id": 11}))
                .await
                .is_ok()
        );
        assert_eq!(
            call(&g, &ana(), "alice", "get", &json!({"id": 12}))
                .await
                .unwrap_err(),
            "document not found",
            "another user's document looks like no document"
        );
        let household = Scope {
            owner: HOUSEHOLD.into(),
            topic: "t".into(),
            readable: vec![HOUSEHOLD.into()],
        };
        assert!(
            call(&g, &household, "alice", "get", &json!({"id": 11}))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn upload_takes_only_this_agents_live_reference() {
        let g = gw().await;
        let r = g.refs.issue(
            Attachment {
                agent: "bob".into(),
                owner: "ana".into(),
                message_id: "m".into(),
                attachment_id: "a".into(),
                filename: "x.pdf".into(),
                mime: "application/pdf".into(),
                size: 10,
            },
            std::time::Duration::from_secs(3600),
        );
        let args = json!({"ref": r, "title": "t"});
        assert_eq!(
            call(&g, &ana(), "alice", "upload", &args)
                .await
                .unwrap_err(),
            "unknown or expired attachment reference"
        );
        let r = g.refs.issue(
            Attachment {
                agent: "alice".into(),
                owner: "ana".into(),
                message_id: "m".into(),
                attachment_id: "a".into(),
                filename: "x.exe".into(),
                mime: "application/x-msdownload".into(),
                size: 10,
            },
            std::time::Duration::from_secs(3600),
        );
        assert_eq!(
            call(
                &g,
                &ana(),
                "alice",
                "upload",
                &json!({"ref": r, "title": "t"})
            )
            .await
            .unwrap_err(),
            "file type not allowed"
        );
    }

    #[test]
    fn cut_respects_char_boundaries() {
        assert_eq!(cut("abc", 5), "abc");
        assert_eq!(cut("ééé", 3), "é… [cut]");
    }
}
