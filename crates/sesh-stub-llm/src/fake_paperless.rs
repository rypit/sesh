//! Fake Paperless-ngx for the eval project (specs/005 research.md R8): just
//! the API calls the gateway's docs connector makes. Consumption finishes
//! at once. It returns every document regardless of owner, so the gateway's
//! own filtering is what the eval relies on. Development and CI only.

use serde_json::{Value, json};
use std::collections::HashMap;

pub struct Fake {
    users: Vec<(u64, String)>,
    tags: Vec<(u64, String)>,
    docs: Vec<Value>,
    tasks: HashMap<String, u64>,
}

impl Default for Fake {
    fn default() -> Self {
        Fake {
            users: vec![(1, "sesh-gateway".into())],
            tags: vec![],
            docs: vec![],
            tasks: HashMap::new(),
        }
    }
}

fn query(url: &str) -> HashMap<String, String> {
    url.split_once('?')
        .map(|(_, q)| {
            q.split('&')
                .filter_map(|kv| kv.split_once('='))
                .map(|(k, v)| (k.to_string(), v.replace('+', " ").replace("%20", " ")))
                .collect()
        })
        .unwrap_or_default()
}

/// A multipart field's text value (crude, for the gateway's own forms).
fn field(body: &str, name: &str) -> Option<String> {
    let at = body.find(&format!("name=\"{name}\""))?;
    let rest = &body[at..];
    let start = rest.find("\r\n\r\n")? + 4;
    let end = rest[start..].find("\r\n")?;
    Some(rest[start..start + end].to_string())
}

/// The text of the stub's tiny PDFs: `(...) Tj`.
fn pdf_text(body: &str) -> String {
    body.split(") Tj")
        .filter_map(|p| p.rsplit_once('(').map(|(_, t)| t.to_string()))
        .collect::<Vec<_>>()
        .join(" ")
}

impl Fake {
    pub fn handle(&mut self, method: &str, url: &str, body: &str) -> Option<(u16, Value)> {
        let path = url.split('?').next().unwrap_or("");
        if !path.starts_with("/api/") {
            return None;
        }
        let q = query(url);
        Some(match (method, path) {
            ("POST", "/api/token/") => (200, json!({"token": "fake-paperless-token"})),
            ("GET", "/api/users/") => {
                let want = q.get("username__iexact").cloned().unwrap_or_default();
                let r: Vec<Value> = self
                    .users
                    .iter()
                    .filter(|(_, n)| n.eq_ignore_ascii_case(&want))
                    .map(|(id, n)| json!({"id": id, "username": n}))
                    .collect();
                (200, json!({"results": r}))
            }
            ("POST", "/api/users/") => {
                let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
                let id = self.users.len() as u64 + 1;
                self.users
                    .push((id, v["username"].as_str().unwrap_or("").into()));
                (201, json!({"id": id}))
            }
            ("GET", "/api/tags/") => {
                let want = q.get("name__iexact").cloned().unwrap_or_default();
                let r: Vec<Value> = self
                    .tags
                    .iter()
                    .filter(|(_, n)| n.eq_ignore_ascii_case(&want))
                    .map(|(id, n)| json!({"id": id, "name": n}))
                    .collect();
                (200, json!({"results": r}))
            }
            ("POST", "/api/tags/") => {
                let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
                let id = self.tags.len() as u64 + 1;
                self.tags
                    .push((id, v["name"].as_str().unwrap_or("").into()));
                (201, json!({"id": id}))
            }
            ("POST", "/api/documents/post_document/") => {
                let id = self.docs.len() as u64 + 1;
                let title = field(body, "title").unwrap_or_else(|| "untitled".into());
                self.docs.push(
                    json!({"id": id, "owner": 1, "title": title, "content": pdf_text(body),
                    "tags": [], "created": "2026-11-27"}),
                );
                let task = format!("task-{id}");
                self.tasks.insert(task.clone(), id);
                (200, json!(task))
            }
            ("GET", "/api/tasks/") => match q.get("task_id").and_then(|t| self.tasks.get(t)) {
                Some(id) => (
                    200,
                    json!([{"status": "SUCCESS", "related_document": id.to_string()}]),
                ),
                None => (200, json!([])),
            },
            ("GET", "/api/documents/") => {
                let needle = q.get("query").cloned().unwrap_or_default().to_lowercase();
                let r: Vec<Value> = self
                    .docs
                    .iter()
                    .filter(|d| {
                        needle.split_whitespace().any(|w| {
                            format!("{} {}", d["title"], d["content"])
                                .to_lowercase()
                                .contains(w)
                        }) || needle.is_empty()
                    })
                    .cloned()
                    .collect();
                (200, json!({"count": r.len(), "results": r}))
            }
            (m, p) if p.starts_with("/api/documents/") => {
                let id: u64 = p
                    .trim_start_matches("/api/documents/")
                    .trim_end_matches('/')
                    .parse()
                    .unwrap_or(0);
                let Some(d) = self.docs.iter_mut().find(|d| d["id"] == id) else {
                    return Some((404, json!({"detail": "Not found."})));
                };
                if m == "PATCH" {
                    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
                    if let Some(o) = v["owner"].as_u64() {
                        d["owner"] = json!(o);
                    }
                }
                (200, d.clone())
            }
            _ => (404, json!({"detail": "Not found."})),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_then_find_and_reown() {
        let mut f = Fake::default();
        let pdf = String::from_utf8(crate::fake_google::pdf("Electricity bill 0915")).unwrap();
        let body = format!(
            "--b\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nBill\r\n--b\r\nContent-Disposition: form-data; name=\"document\"; filename=\"x.pdf\"\r\n\r\n{pdf}\r\n--b--\r\n"
        );
        let (_, task) = f
            .handle("POST", "/api/documents/post_document/", &body)
            .unwrap();
        let (_, t) = f
            .handle(
                "GET",
                &format!("/api/tasks/?task_id={}", task.as_str().unwrap()),
                "",
            )
            .unwrap();
        assert_eq!(t[0]["related_document"], "1");
        f.handle("PATCH", "/api/documents/1/", r#"{"owner": 2}"#)
            .unwrap();
        let (_, d) = f.handle("GET", "/api/documents/?query=0915", "").unwrap();
        assert_eq!(d["results"][0]["owner"], 2);
        assert_eq!(d["results"][0]["title"], "Bill");
        let (_, u) = f
            .handle("POST", "/api/users/", r#"{"username": "sesh-household"}"#)
            .unwrap();
        assert_eq!(u["id"], 2);
        assert_eq!(f.handle("GET", "/api/documents/9/", "").unwrap().0, 404);
    }
}
