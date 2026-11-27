//! Fake Google (specs/005 research.md R7): OAuth token and revoke, and
//! just enough Gmail, Calendar and People for the gateway's connectors.
//! Each user has their own mailbox, keyed by the tokens this fake issued,
//! so a test can tell whose data a call reached. Development and CI only.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};
use std::collections::HashMap;

pub const ALL_SCOPES: &str = "https://www.googleapis.com/auth/gmail.readonly https://www.googleapis.com/auth/gmail.compose https://www.googleapis.com/auth/calendar.events https://www.googleapis.com/auth/contacts.readonly";

#[derive(Default)]
pub struct Fake {
    n: u64,
    /// refresh token → user
    refresh: HashMap<String, String>,
    /// access token → user
    access: HashMap<String, String>,
    /// sent mail, drafts and created events, by user (for tests to read)
    sent: HashMap<String, Vec<Value>>,
    drafts: HashMap<String, Vec<Value>>,
    events: HashMap<String, Vec<Value>>,
}

/// A small, valid one-page PDF whose text is `text` (offsets computed).
pub fn pdf(text: &str) -> Vec<u8> {
    let stream = format!("BT /F1 18 Tf 72 720 Td ({text}) Tj ET");
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = vec![];
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{o}\nendobj\n", i + 1));
    }
    let xref = out.len();
    out.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objs.len() + 1
    ));
    for o in offsets {
        out.push_str(&format!("{o:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objs.len() + 1
    ));
    out.into_bytes()
}

struct Msg {
    id: &'static str,
    from: &'static str,
    subject: &'static str,
    body: &'static str,
    attachment: Option<(&'static str, &'static str)>,
}

/// Each user's fixed mailbox. Facts are distinctive so a test can tell
/// whose mail an answer came from.
fn mailbox(user: &str) -> Vec<Msg> {
    match user {
        "ana" => vec![Msg {
            id: "m-ana-1",
            from: "Clinic <clinic@example.org>",
            subject: "Your dentist appointment",
            body: "Your appointment is on Tuesday at 10:40 with Dr. Okafor. Invoice attached.",
            attachment: Some(("a-ana-1", "dentist-invoice.pdf")),
        }],
        "ben" => vec![Msg {
            id: "m-ben-1",
            from: "Bank <bank@example.org>",
            subject: "Ben's statement",
            body: "Ben's private code word is marigold-7731.",
            attachment: None,
        }],
        "household" => vec![Msg {
            id: "m-hh-1",
            from: "Utility <bills@example.org>",
            subject: "Household electricity bill",
            body: "The electricity bill for November is 412 kroner, due on the 15th.",
            attachment: Some(("a-hh-1", "electricity-bill.pdf")),
        }],
        _ => vec![],
    }
}

fn attachment_text(id: &str) -> &'static str {
    match id {
        "a-ana-1" => "Dentist invoice 4471 for Ana",
        _ => "Electricity bill 0915 for the household",
    }
}

fn payload(m: &Msg) -> Value {
    let headers = json!([
        {"name": "From", "value": m.from},
        {"name": "To", "value": "me@example.org"},
        {"name": "Subject", "value": m.subject},
        {"name": "Date", "value": "Thu, 26 Nov 2026 09:00:00 +0000"},
    ]);
    let mut parts = vec![
        json!({"mimeType": "text/plain", "body": {"data": URL_SAFE_NO_PAD.encode(m.body), "size": m.body.len()}}),
    ];
    if let Some((aid, name)) = m.attachment {
        parts.push(json!({"mimeType": "application/pdf", "filename": name,
            "body": {"attachmentId": aid, "size": pdf(attachment_text(aid)).len()}}));
    }
    json!({"mimeType": "multipart/mixed", "headers": headers, "parts": parts})
}

pub struct Reply {
    pub status: u16,
    pub body: Value,
}

fn ok(body: Value) -> Reply {
    Reply { status: 200, body }
}

fn err(status: u16, e: &str) -> Reply {
    Reply {
        status,
        body: json!({"error": {"code": status, "message": e}}),
    }
}

fn form(body: &str) -> HashMap<String, String> {
    body.split('&')
        .filter_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            Some((k.to_string(), decode_pct(v)))
        })
        .collect()
}

fn decode_pct(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                if let Ok(x) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(x);
                    i += 2;
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query(url: &str) -> HashMap<String, String> {
    url.split_once('?')
        .map(|(_, q)| form(q))
        .unwrap_or_default()
}

/// Headers of a raw RFC 822 message (base64url), for tests to inspect.
fn summarize_raw(raw: &str) -> Value {
    let bytes = URL_SAFE_NO_PAD
        .decode(raw.trim_end_matches('='))
        .unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes);
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let mut v = json!({"body": body});
    for line in head.lines() {
        if let Some((k, val)) = line.split_once(": ") {
            v[k.to_ascii_lowercase()] = json!(val);
        }
    }
    v
}

impl Fake {
    fn issue_access(&mut self, user: &str) -> String {
        self.n += 1;
        let t = format!("fat-{user}-{}", self.n);
        self.access.insert(t.clone(), user.into());
        t
    }

    /// Handles a request under `/fake/google`; `None` when not one of ours.
    pub fn handle(&mut self, method: &str, url: &str, auth: &str, body: &str) -> Option<Reply> {
        let path = url.split('?').next().unwrap_or("");
        let path = path.strip_prefix("/fake/google")?;
        if path == "/auth" {
            // A real browser would sign in here; tests call the callback
            // themselves with code `fake-<user>`.
            return Some(ok(
                json!({"fake": "consent", "hint": "call the callback with code=fake-<user>"}),
            ));
        }
        if (method, path) == ("POST", "/token") {
            let f = form(body);
            return Some(match f.get("grant_type").map(String::as_str) {
                Some("authorization_code") => {
                    let code = f.get("code").cloned().unwrap_or_default();
                    let Some(rest) = code.strip_prefix("fake-") else {
                        return Some(Reply {
                            status: 400,
                            body: json!({"error": "invalid_grant"}),
                        });
                    };
                    if f.get("code_verifier").is_none_or(|v| v.len() < 43) {
                        return Some(Reply {
                            status: 400,
                            body: json!({"error": "invalid_request"}),
                        });
                    }
                    // `fake-<user>-readonly` grants only mail reading.
                    let (user, scopes) = match rest.strip_suffix("-readonly") {
                        Some(u) => (
                            u.to_string(),
                            "https://www.googleapis.com/auth/gmail.readonly",
                        ),
                        None => (rest.to_string(), ALL_SCOPES),
                    };
                    self.n += 1;
                    let rt = format!("frt-{user}-{}", self.n);
                    self.refresh.insert(rt.clone(), user.clone());
                    let at = self.issue_access(&user);
                    ok(
                        json!({"access_token": at, "expires_in": 3600, "refresh_token": rt, "scope": scopes, "token_type": "Bearer"}),
                    )
                }
                Some("refresh_token") => {
                    let rt = f.get("refresh_token").cloned().unwrap_or_default();
                    match self.refresh.get(&rt).cloned() {
                        Some(user) => {
                            let at = self.issue_access(&user);
                            ok(
                                json!({"access_token": at, "expires_in": 3600, "token_type": "Bearer"}),
                            )
                        }
                        None => Reply {
                            status: 400,
                            body: json!({"error": "invalid_grant"}),
                        },
                    }
                }
                _ => Reply {
                    status: 400,
                    body: json!({"error": "unsupported_grant_type"}),
                },
            });
        }
        if (method, path) == ("POST", "/revoke") {
            let t = form(body).get("token").cloned().unwrap_or_default();
            if let Some(user) = self.refresh.remove(&t) {
                self.access.retain(|_, u| *u != user);
            }
            return Some(ok(json!({})));
        }
        // Google-side revocation, for tests: forget every grant of a user.
        if (method, path) == ("POST", "/revoke-user") {
            let user = query(url).get("user").cloned().unwrap_or_default();
            self.refresh.retain(|_, u| *u != user);
            self.access.retain(|_, u| *u != user);
            return Some(ok(json!({})));
        }
        if (method, path) == ("GET", "/sent") {
            return Some(ok(
                json!({"sent": self.sent, "drafts": self.drafts, "events": self.events}),
            ));
        }
        let Some(user) = auth
            .strip_prefix("Bearer ")
            .and_then(|t| self.access.get(t))
            .cloned()
        else {
            return Some(err(401, "invalid credentials"));
        };
        let q = query(url);
        let gmail = "/gmail/v1/users/me";
        let mine = mailbox(&user);
        Some(match (method, path) {
            ("GET", p) if p == format!("{gmail}/messages") => {
                let needle = q.get("q").cloned().unwrap_or_default().to_lowercase();
                let ids: Vec<Value> = mine
                    .iter()
                    .filter(|m| {
                        needle.is_empty()
                            || needle.split_whitespace().any(|w| {
                                format!("{} {}", m.subject, m.body)
                                    .to_lowercase()
                                    .contains(w)
                            })
                    })
                    .map(|m| json!({"id": m.id, "threadId": m.id}))
                    .collect();
                ok(json!({"messages": ids, "resultSizeEstimate": ids.len()}))
            }
            ("POST", p) if p == format!("{gmail}/drafts") => {
                let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
                let d = summarize_raw(v["message"]["raw"].as_str().unwrap_or(""));
                let list = self.drafts.entry(user).or_default();
                list.push(d);
                ok(json!({"id": format!("d-{}", list.len())}))
            }
            ("POST", p) if p == format!("{gmail}/messages/send") => {
                let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
                let m = summarize_raw(v["raw"].as_str().unwrap_or(""));
                let list = self.sent.entry(user).or_default();
                list.push(m);
                ok(json!({"id": format!("s-{}", list.len())}))
            }
            ("GET", p) if p.starts_with(&format!("{gmail}/messages/")) => {
                let rest = &p[gmail.len() + "/messages/".len()..];
                let (id, att) = match rest.split_once("/attachments/") {
                    Some((i, a)) => (i, Some(a)),
                    None => (rest, None),
                };
                // Another user's message is simply not found.
                let Some(m) = mine.iter().find(|m| m.id == id) else {
                    return Some(err(404, "Requested entity was not found."));
                };
                match att {
                    Some(a) if m.attachment.is_some_and(|(x, _)| x == a) => {
                        let bytes = pdf(attachment_text(a));
                        ok(json!({"data": URL_SAFE_NO_PAD.encode(&bytes), "size": bytes.len()}))
                    }
                    Some(_) => err(404, "Requested entity was not found."),
                    None => ok(
                        json!({"id": m.id, "snippet": m.body.chars().take(60).collect::<String>(), "payload": payload(m)}),
                    ),
                }
            }
            ("GET", "/calendar/v3/calendars/primary/events") => {
                let mut items = vec![
                    json!({"id": format!("e-{user}-1"), "summary": format!("{user} standing meeting"),
                    "start": {"dateTime": "2026-11-30T09:00:00Z"}, "end": {"dateTime": "2026-11-30T09:30:00Z"}}),
                ];
                items.extend(self.events.get(&user).cloned().unwrap_or_default());
                ok(json!({"items": items}))
            }
            ("POST", "/calendar/v3/calendars/primary/events") => {
                let mut v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
                let list = self.events.entry(user.clone()).or_default();
                v["id"] = json!(format!("e-{user}-new-{}", list.len() + 1));
                list.push(v.clone());
                ok(json!({"id": v["id"]}))
            }
            ("GET", "/v1/people:searchContacts") => ok(json!({"results": [
                {"person": {"names": [{"displayName": "Dr. Okafor"}], "emailAddresses": [{"value": "clinic@example.org"}]}}
            ]})),
            _ => err(404, "not found"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(f: &mut Fake, code: &str) -> Value {
        let body = format!(
            "grant_type=authorization_code&code={code}&code_verifier={}",
            "v".repeat(64)
        );
        f.handle("POST", "/fake/google/token", "", &body)
            .unwrap()
            .body
    }

    #[test]
    fn each_user_reaches_only_their_mailbox() {
        let mut f = Fake::default();
        let ana = token(&mut f, "fake-ana");
        let ben = token(&mut f, "fake-ben");
        let auth = |t: &Value| format!("Bearer {}", t["access_token"].as_str().unwrap());
        let list = f
            .handle(
                "GET",
                "/fake/google/gmail/v1/users/me/messages?q=dentist",
                &auth(&ana),
                "",
            )
            .unwrap();
        assert_eq!(list.body["messages"][0]["id"], "m-ana-1");
        let other = f
            .handle(
                "GET",
                "/fake/google/gmail/v1/users/me/messages/m-ben-1?format=full",
                &auth(&ana),
                "",
            )
            .unwrap();
        assert_eq!(other.status, 404);
        let own = f
            .handle(
                "GET",
                "/fake/google/gmail/v1/users/me/messages/m-ben-1?format=full",
                &auth(&ben),
                "",
            )
            .unwrap();
        assert_eq!(own.status, 200);
        assert_eq!(
            f.handle(
                "GET",
                "/fake/google/gmail/v1/users/me/messages",
                "Bearer nope",
                ""
            )
            .unwrap()
            .status,
            401
        );
        // Revoking the refresh token ends refreshing.
        let rt = ana["refresh_token"].as_str().unwrap().to_string();
        f.handle("POST", "/fake/google/revoke", "", &format!("token={rt}"))
            .unwrap();
        let r = f
            .handle(
                "POST",
                "/fake/google/token",
                "",
                &format!("grant_type=refresh_token&refresh_token={rt}"),
            )
            .unwrap();
        assert_eq!(r.body["error"], "invalid_grant");
    }

    #[test]
    fn send_records_the_message_headers() {
        let mut f = Fake::default();
        let t = token(&mut f, "fake-ana");
        let raw = URL_SAFE_NO_PAD.encode("To: x@example.org\r\nSubject: Hi\r\n\r\nHello");
        let auth = format!("Bearer {}", t["access_token"].as_str().unwrap());
        f.handle(
            "POST",
            "/fake/google/gmail/v1/users/me/messages/send",
            &auth,
            &json!({"raw": raw}).to_string(),
        )
        .unwrap();
        let s = f.handle("GET", "/fake/google/sent", "", "").unwrap().body;
        assert_eq!(s["sent"]["ana"][0]["subject"], "Hi");
        assert_eq!(s["sent"]["ana"][0]["to"], "x@example.org");
    }

    #[test]
    fn the_pdf_is_well_formed() {
        let p = String::from_utf8(pdf("Dentist invoice 4471")).unwrap();
        assert!(p.starts_with("%PDF-1.4") && p.ends_with("%%EOF\n"));
        let xref: usize = p
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(p[xref..].starts_with("xref"));
    }
}
