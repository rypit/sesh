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
                    "File a file, by a reference mail.read, browser.downloads or a click's download gave you, into your owner's documents.",
                    json!({"type": "object", "required": ["ref", "title"], "properties": {"ref": s(32, 32), "title": s(1, 200),
                    "tags": json!({"type": "array", "items": s(1, 50), "maxItems": 10})}}),
                    false,
                ),
            ),
            (
                "ref",
                super::method(
                    "A reference to one of your documents, to upload it with browser.upload (the file itself never comes back).",
                    json!({"type": "object", "required": ["id"], "properties": {"id": json!({"type": "integer", "minimum": 1, "maximum": 1_000_000_000})}}),
                    false,
                ),
            ),
        ],
    )
}

/// A document's original file, by id (the caller checked the owner when
/// it issued the reference: specs/010 R9).
pub async fn document_bytes(gw: &Gateway, _owner: &str, id: u64) -> Result<Vec<u8>, String> {
    let base = gw
        .settings
        .upstreams
        .paperless
        .trim_end_matches('/')
        .to_string();
    gw.paperless.original(&base, id).await
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

    /// A Paperless user's id by exact name.
    async fn find_user(&self, base: &str, name: &str) -> Result<Option<u64>, String> {
        let found = self
            .req(
                base,
                reqwest::Method::GET,
                "/api/users/",
                &[("username__iexact", name.to_string())],
                None,
            )
            .await?;
        // Match the name exactly here: a filter Paperless ignored would
        // otherwise hand back the first user (the admin).
        Ok(found["results"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|u| {
                u["username"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .and_then(|u| u["id"].as_u64()))
    }

    /// The Paperless user id for an owner, created the first time.
    pub async fn owner_id(&self, base: &str, owner: &str) -> Result<u64, String> {
        if let Some(id) = self.ids.lock().await.get(owner) {
            return Ok(*id);
        }
        let name = format!("sesh-{owner}");
        let exact = self.find_user(base, &name).await?;
        let id = match exact {
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
            let exact = found["results"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|x| {
                    x["name"]
                        .as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(t))
                })
                .and_then(|x| x["id"].as_u64());
            let id = match exact {
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

    /// A document's original file.
    async fn original(&self, base: &str, id: u64) -> Result<Vec<u8>, String> {
        let tok = self.token(base).await?;
        let r = self
            .http
            .get(format!("{base}/api/documents/{id}/download/?original=true"))
            .header("Authorization", format!("Token {tok}"))
            .send()
            .await
            .map_err(|_| "the document service is unavailable".to_string())?;
        if !r.status().is_success() {
            return Err("document not found".into());
        }
        Ok(r.bytes()
            .await
            .map_err(|_| "the document service sent an unreadable reply".to_string())?
            .to_vec())
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

    /// The newest document still owned by the gateway's own account (so
    /// not yet given to anyone) with this upload's exact title and file
    /// name. Only unassigned uploads can match, never someone's document.
    async fn find_unassigned(&self, base: &str, up: &Pending) -> Result<Option<u64>, String> {
        let Some(me) = self.find_user(base, &self.user).await? else {
            return Ok(None);
        };
        let v = self
            .req(
                base,
                reqwest::Method::GET,
                "/api/documents/",
                &[
                    ("title__iexact", up.title.clone()),
                    ("owner__id", me.to_string()),
                    ("ordering", "-added".into()),
                    ("page_size", "10".into()),
                ],
                None,
            )
            .await?;
        Ok(v["results"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|d| {
                d["owner"].as_u64() == Some(me)
                    && d["title"].as_str() == Some(up.title.as_str())
                    && d["original_file_name"]
                        .as_str()
                        .is_none_or(|f| f == up.filename)
            })
            .and_then(|d| d["id"].as_u64()))
    }

    /// Waits for consumption, then gives the document its owner. Until
    /// then it belongs to the gateway's admin account, which no agent's
    /// scope includes, so it is invisible rather than misfiled.
    async fn finish(
        &self,
        base: &str,
        up: &Pending,
        owner_id: u64,
        polls: u32,
    ) -> Result<Option<u64>, String> {
        let task = up.task.clone();
        let mut seen = String::new();
        let mut finished_without_id = 0;
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
            // For the log if this never finishes: the status words only.
            seen = t
                .as_array()
                .or(t["results"].as_array())
                .map(|a| {
                    a.iter()
                        .map(|x| {
                            format!(
                                "{}:{}",
                                x["task_id"].as_str() == Some(task.as_str()),
                                x["status"].as_str().unwrap_or("?")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_else(|| "no list".into());
            match task_state(&t, &task) {
                TaskState::Done(id) => {
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
                TaskState::Failed(why) => {
                    return Err(format!(
                        "the document service couldn't process the file: {why}"
                    ));
                }
                TaskState::Finished(entry) => {
                    // Done, but this Paperless didn't say which document:
                    // allow a moment (the result can land just after the
                    // status), then find it by its title and file name.
                    finished_without_id += 1;
                    if finished_without_id == 4 {
                        tracing::warn!(entry = %entry, "task finished without a document id; looking it up");
                    }
                    if finished_without_id >= 4
                        && let Some(id) = self.find_unassigned(base, up).await?
                    {
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
                    tokio::time::sleep(Duration::from_millis(500)).await
                }
                TaskState::Waiting => tokio::time::sleep(Duration::from_millis(500)).await,
            }
        }
        tracing::warn!(polls, seen = %seen.chars().take(200).collect::<String>(), "a document is still being consumed");
        Ok(None)
    }
}

/// An upload Paperless is consuming.
#[derive(Clone)]
struct Pending {
    task: String,
    title: String,
    filename: String,
}

#[derive(Debug, PartialEq)]
enum TaskState {
    Waiting,
    Done(u64),
    /// Succeeded without naming the document (the entry, cut, for the log).
    Finished(String),
    Failed(String),
}

/// Reads Paperless's task list for `task`. Versions differ: the list may
/// be bare or paged, the status upper or lower case, and the new document
/// in `related_document` or only in `result` (`{'document_id': 1}`, or
/// "... New document id 1 created"). An entry for another task is ignored.
fn task_state(list: &Value, task: &str) -> TaskState {
    let items = list
        .as_array()
        .or(list["results"].as_array())
        .cloned()
        .unwrap_or_default();
    let Some(t) = items
        .iter()
        .find(|t| t["task_id"].as_str() == Some(task))
        .or(if items.len() == 1 && items[0]["task_id"].is_null() {
            items.first()
        } else {
            None
        })
    else {
        return TaskState::Waiting;
    };
    let digits_after = |s: &str, key: &str| -> Option<u64> {
        let at = s.find(key)? + key.len();
        let rest: String = s[at..]
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        rest.parse().ok()
    };
    let result = match &t["result"] {
        Value::String(x) => x.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    match t["status"]
        .as_str()
        .unwrap_or("")
        .to_ascii_uppercase()
        .as_str()
    {
        "SUCCESS" => {
            let id = t["related_document"]
                .as_u64()
                .or(t["related_document"].as_str().and_then(|x| x.parse().ok()))
                .or(t["result"]["document_id"].as_u64())
                .or(digits_after(&result, "document_id"))
                .or(digits_after(&result, "document id"));
            match id {
                Some(id) => TaskState::Done(id),
                None => TaskState::Finished(
                    format!(
                        "status {} result {} related {}",
                        t["status"], result, t["related_document"]
                    )
                    .chars()
                    .take(300)
                    .collect(),
                ),
            }
        }
        "FAILURE" | "REVOKED" => TaskState::Failed(result.chars().take(200).collect()),
        _ => TaskState::Waiting,
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
            let q = args["query"].as_str().unwrap_or("").to_string();
            let size = args["limit"].as_u64().unwrap_or(10).to_string();
            let find = |key: &'static str| {
                let (q, ids, size) = (q.clone(), ids.join(","), size.clone());
                let base = base.clone();
                async move {
                    p.req(
                        &base,
                        reqwest::Method::GET,
                        "/api/documents/",
                        &[(key, q), ("owner__id__in", ids), ("page_size", size)],
                        None,
                    )
                    .await
                }
            };
            // Full-text first; a document consumed moments ago may not be
            // in the index yet, so fall back to the title/content filter.
            let mut v = find("query").await?;
            if v["results"].as_array().is_none_or(|r| r.is_empty()) {
                v = find("title_content").await?;
            }
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
            let bytes = super::file_bytes(gw, &att).await?;
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
            let up = Pending {
                task,
                title: title.clone(),
                filename: att.filename.clone(),
            };
            match p.finish(&base, &up, owner_id, INLINE_POLLS).await? {
                Some(id) => Ok(json!({"id": id, "title": title, "status": "filed"})),
                None => {
                    let (p, base) = (p.clone(), base.clone());
                    tokio::spawn(async move {
                        if let Err(e) = p.finish(&base, &up, owner_id, BACKGROUND_POLLS).await {
                            tracing::warn!("filing a document failed: {e}");
                        }
                    });
                    Ok(
                        json!({"title": title, "status": "processing", "note": "the document service is still reading the file; it will be searchable shortly"}),
                    )
                }
            }
        }
        "ref" => {
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
            let filename = d["original_file_name"]
                .as_str()
                .or(d["title"].as_str())
                .unwrap_or("document")
                .to_string();
            let mime = d["mime_type"]
                .as_str()
                .unwrap_or("application/octet-stream")
                .to_string();
            let a = crate::refs::Attachment {
                agent: agent.into(),
                owner: scope.owner.clone(),
                source: crate::refs::Source::Document { id },
                filename: filename.clone(),
                mime: mime.clone(),
                size: d["original_size"].as_u64().unwrap_or(0),
            };
            let size = a.size;
            let r = gw
                .refs
                .issue(a, std::time::Duration::from_secs(gw.settings.ref_ttl_secs));
            Ok(json!({"ref": r, "filename": filename, "mime": mime, "size": size}))
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
                // Ben's document and the gateway's own unassigned upload,
                // with the same title.
                json!({"id": 14, "owner": 3, "title": "Pending bill", "original_file_name": "bill.pdf", "content": "", "tags": [], "created": "2026-01-01"}),
                json!({"id": 13, "owner": 99, "title": "Pending bill", "original_file_name": "bill.pdf", "content": "", "tags": [], "created": "2026-01-01"}),
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
                    let name = q.get("username__iexact").cloned().unwrap_or_default();
                    let id = match name.as_str() {
                        "sesh-household" => 1,
                        "sesh-ana" => 2,
                        "sesh-ben" => 3,
                        "sesh-gateway" => 99,
                        _ => return Json(json!({"results": []})),
                    };
                    // The admin first, as if the filter were ignored: the
                    // gateway must pick the exact name.
                    Json(json!({"results": [{"id": 99, "username": "sesh-gateway"}, {"id": id, "username": name}]}))
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
    async fn a_nameless_finished_upload_is_found_among_unassigned_only() {
        let g = gw().await;
        let base = g.settings.upstreams.paperless.clone();
        let up = |title: &str| Pending {
            task: "t".into(),
            title: title.into(),
            filename: "bill.pdf".into(),
        };
        assert_eq!(
            g.paperless
                .find_unassigned(&base, &up("Pending bill"))
                .await
                .unwrap(),
            Some(13)
        );
        assert_eq!(
            g.paperless
                .find_unassigned(&base, &up("Nothing like it"))
                .await
                .unwrap(),
            None
        );
        let other_file = Pending {
            filename: "other.pdf".into(),
            ..up("Pending bill")
        };
        assert_eq!(
            g.paperless
                .find_unassigned(&base, &other_file)
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn upload_takes_only_this_agents_live_reference() {
        let g = gw().await;
        let r = g.refs.issue(
            Attachment {
                agent: "bob".into(),
                owner: "ana".into(),
                source: crate::refs::Source::Gmail {
                    message_id: "m".into(),
                    attachment_id: "a".into(),
                },
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
                source: crate::refs::Source::Gmail {
                    message_id: "m".into(),
                    attachment_id: "a".into(),
                },
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
    fn task_state_reads_each_paperless_shape() {
        let t = "abc";
        assert_eq!(task_state(&json!([]), t), TaskState::Waiting);
        assert_eq!(
            task_state(&json!([{"task_id": "abc", "status": "STARTED"}]), t),
            TaskState::Waiting
        );
        assert_eq!(
            task_state(
                &json!([{"task_id": "abc", "status": "SUCCESS", "related_document": "7"}]),
                t
            ),
            TaskState::Done(7)
        );
        assert_eq!(
            task_state(
                &json!({"results": [{"task_id": "abc", "status": "success", "result": {"document_id": 8}}]}),
                t
            ),
            TaskState::Done(8)
        );
        assert_eq!(
            task_state(
                &json!([{"task_id": "abc", "status": "SUCCESS", "result": "{'document_id': 9}"}]),
                t
            ),
            TaskState::Done(9)
        );
        assert_eq!(
            task_state(
                &json!([{"task_id": "abc", "status": "SUCCESS", "result": "Success. New document id 10 created"}]),
                t
            ),
            TaskState::Done(10)
        );
        // Another task's entry (a filter Paperless ignored) is not ours.
        assert_eq!(
            task_state(
                &json!([{"task_id": "zzz", "status": "SUCCESS", "related_document": 1}]),
                t
            ),
            TaskState::Waiting
        );
        assert!(matches!(
            task_state(
                &json!([{"task_id": "abc", "status": "SUCCESS", "result": null}]),
                t
            ),
            TaskState::Finished(_)
        ));
        assert!(matches!(
            task_state(&json!([{"task_id": "abc", "status": "FAILURE", "result": "bad pdf"}]), t),
            TaskState::Failed(w) if w == "bad pdf"
        ));
    }

    #[test]
    fn cut_respects_char_boundaries() {
        assert_eq!(cut("abc", 5), "abc");
        assert_eq!(cut("ééé", 3), "é… [cut]");
    }
}
