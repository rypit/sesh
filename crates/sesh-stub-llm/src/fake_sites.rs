//! Fake web sites for the browser's stack test (specs/010 research.md R8),
//! answered by host name (Compose aliases of the stub on the `stub`
//! network, reached through the gateway's egress proxy):
//!
//! - `fake-news.test`: an article, a redirect to the home network, a page
//!   whose image is on the home network;
//! - `fake-bank.test`: a login (ana / hunter2-x), an account page with a
//!   statement (PDF download), a setup.exe and an oversized PDF;
//! - `fake-shop.test`: an order form (a submit needs approval) and an
//!   upload form.
//!
//! `GET /fake/sites/seen` (any host) reports what the sites received:
//! login attempts (never the password itself: only whether it matched),
//! orders and uploads (name, size and SHA-256).

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const HOSTS: [&str; 3] = ["fake-news.test", "fake-bank.test", "fake-shop.test"];

pub struct Reply {
    pub status: u16,
    pub content_type: &'static str,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

#[derive(Default)]
pub struct Fake {
    seen: Vec<Value>,
}

fn page(title: &str, body: &str) -> Reply {
    Reply {
        status: 200,
        content_type: "text/html; charset=utf-8",
        headers: vec![],
        body: format!(
            "<!doctype html><html><head><title>{title}</title></head><body>{body}</body></html>"
        )
        .into_bytes(),
    }
}

fn redirect(to: &str, cookie: Option<&str>) -> Reply {
    let mut headers = vec![("Location", to.to_string())];
    if let Some(c) = cookie {
        headers.push(("Set-Cookie", c.to_string()));
    }
    Reply {
        status: 302,
        content_type: "text/plain",
        headers,
        body: vec![],
    }
}

fn file(ct: &'static str, name: &str, body: Vec<u8>) -> Reply {
    Reply {
        status: 200,
        content_type: ct,
        headers: vec![(
            "Content-Disposition",
            format!("attachment; filename=\"{name}\""),
        )],
        body,
    }
}

fn form_fields(body: &[u8]) -> Vec<(String, String)> {
    url::form_urlencoded::parse(body)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// The file parts of a multipart body: (field, filename, bytes).
fn multipart_files(content_type: &str, body: &[u8]) -> Vec<(String, String, Vec<u8>)> {
    let Some(b) = content_type.split("boundary=").nth(1) else {
        return vec![];
    };
    let sep = format!("--{}", b.trim_matches('"'));
    let text = body;
    let mut out = vec![];
    let sepb = sep.as_bytes();
    let mut starts = vec![];
    let mut i = 0;
    while i + sepb.len() <= text.len() {
        if &text[i..i + sepb.len()] == sepb {
            starts.push(i);
            i += sepb.len();
        } else {
            i += 1;
        }
    }
    for w in starts.windows(2) {
        let part = &text[w[0] + sepb.len()..w[1]];
        let Some(h_end) = part.windows(4).position(|x| x == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&part[..h_end])
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("content-disposition"))
            .unwrap_or("")
            .to_string();
        let mut data = part[h_end + 4..].to_vec();
        if data.ends_with(b"\r\n") {
            data.truncate(data.len() - 2);
        }
        let attr = |k: &str| {
            head.split(';')
                .map(str::trim)
                .find_map(|p| p.strip_prefix(&format!("{k}=")))
                .map(|v| v.trim_matches('"').to_string())
        };
        if let (Some(name), Some(filename)) = (attr("name"), attr("filename")) {
            out.push((name, filename, data));
        }
    }
    out
}

impl Fake {
    pub fn seen(&self) -> Value {
        json!(self.seen)
    }

    /// Answers a request to one of the fake sites.
    pub fn handle(
        &mut self,
        host: &str,
        method: &str,
        path: &str,
        cookie: &str,
        content_type: &str,
        body: &[u8],
    ) -> Option<Reply> {
        let host = host.split(':').next().unwrap_or("");
        let signed_in = cookie.split(';').any(|c| c.trim() == "session=ana-ok");
        Some(match (host, method, path) {
            ("fake-news.test", "GET", "/article") => page(
                "Harbour news",
                "<nav>Menu: home, sport</nav><article><h1>The Varden beacon</h1><p>The beacon on Skarvoy was first lit in 1873.</p><p><a href='http://fake-shop.test:8000/order'>Order the book</a></p></article>",
            ),
            ("fake-news.test", "GET", "/redirect") => redirect("http://10.0.0.1/secret", None),
            ("fake-news.test", "GET", "/sub") => page(
                "Sub",
                "<p>A page with a picture from the home network.</p><img src='http://192.168.1.1/router.png'>",
            ),
            ("fake-bank.test", "GET", "/login") => page(
                "Fake Bank",
                "<form method='post' action='/login'><label for='u'>Email</label><input id='u' name='username' type='email'><label for='p'>Password</label><input id='p' name='password' type='password'><button>Sign in</button></form>",
            ),
            ("fake-bank.test", "POST", "/login") => {
                let f = form_fields(body);
                let get = |k: &str| {
                    f.iter()
                        .find(|(n, _)| n == k)
                        .map(|(_, v)| v.as_str())
                        .unwrap_or("")
                };
                let ok = get("username") == "ana@example.org" && get("password") == "hunter2-x";
                self.seen.push(json!({"kind": "login", "site": host, "user": get("username"), "password_matched": ok}));
                if ok {
                    redirect("/account", Some("session=ana-ok; Path=/; HttpOnly"))
                } else {
                    page(
                        "Fake Bank",
                        "<p>Wrong login.</p><form method='post' action='/login'><input name='username' type='email'><input name='password' type='password'><button>Sign in</button></form>",
                    )
                }
            }
            ("fake-bank.test", "GET", "/account") if signed_in => page(
                "Your account",
                "<p>Welcome back, Ana. Balance: 412 kroner.</p><a href='/statement'>Statement</a> <a href='/setup.exe'>Setup</a> <a href='/big.pdf'>Archive</a>",
            ),
            ("fake-bank.test", "GET", "/account") => redirect("/login", None),
            ("fake-bank.test", "GET", "/statement") if signed_in => file(
                "application/pdf",
                "statement-november.pdf",
                // A real PDF, so Paperless can file it (FR-018).
                crate::fake_google::pdf("Fake Bank statement for Ana, November: 412 kroner"),
            ),
            ("fake-bank.test", "GET", "/setup.exe") => {
                file("application/x-msdownload", "setup.exe", b"MZ fake".to_vec())
            }
            ("fake-bank.test", "GET", "/big.pdf") => {
                let mut b = b"%PDF-1.4\n".to_vec();
                b.resize(26 * 1024 * 1024, b' ');
                file("application/pdf", "archive.pdf", b)
            }
            ("fake-shop.test", "GET", "/order") => page(
                "Fake Shop",
                "<form method='post' action='/order'><label for='n'>Name</label><input id='n' name='name'><button>Place order</button></form><p><a href='/upload'>Send us a document</a></p>",
            ),
            ("fake-shop.test", "POST", "/order") => {
                let f = form_fields(body);
                let name = f
                    .iter()
                    .find(|(n, _)| n == "name")
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();
                self.seen.push(json!({"kind": "order", "name": name}));
                page("Thanks", "<p>Order placed.</p>")
            }
            ("fake-shop.test", "GET", "/upload") => page(
                "Fake Shop",
                "<form method='post' action='/upload' enctype='multipart/form-data'><label>Your document <input type='file' name='doc'></label><button>Send</button></form>",
            ),
            ("fake-shop.test", "POST", "/upload") => {
                for (_, filename, data) in multipart_files(content_type, body) {
                    let sha: String = Sha256::digest(&data)
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect();
                    self.seen.push(json!({"kind": "upload", "filename": filename, "size": data.len(), "sha256": sha}));
                }
                page("Thanks", "<p>Received.</p>")
            }
            (h, _, _) if HOSTS.contains(&h) => Reply {
                status: 404,
                content_type: "text/plain",
                headers: vec![],
                body: b"not found".to_vec(),
            },
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bank_signs_in_only_with_the_right_login_and_never_records_it() {
        let mut f = Fake::default();
        let r = f
            .handle(
                "fake-bank.test:8000",
                "POST",
                "/login",
                "",
                "",
                b"username=ana%40example.org&password=wrong",
            )
            .unwrap();
        assert_eq!(r.status, 200);
        let r = f
            .handle(
                "fake-bank.test",
                "POST",
                "/login",
                "",
                "",
                b"username=ana%40example.org&password=hunter2-x",
            )
            .unwrap();
        assert_eq!(r.status, 302);
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| *k == "Set-Cookie" && v.contains("session=ana-ok"))
        );
        let seen = f.seen().to_string();
        assert!(
            !seen.contains("hunter2-x") && !seen.contains("wrong"),
            "{seen}"
        );
        assert!(seen.contains("\"password_matched\":true"));
        assert_eq!(
            f.handle(
                "fake-bank.test",
                "GET",
                "/statement",
                "session=ana-ok",
                "",
                b""
            )
            .unwrap()
            .status,
            200
        );
        assert!(
            f.handle("stub-llm", "GET", "/statement", "", "", b"")
                .is_none(),
            "other hosts aren't sites"
        );
    }

    #[test]
    fn uploads_are_recorded_by_size_and_hash() {
        let mut f = Fake::default();
        let body = b"--XYZ\r\nContent-Disposition: form-data; name=\"doc\"; filename=\"u.pdf\"\r\nContent-Type: application/pdf\r\n\r\n%PDF data\r\n--XYZ--\r\n";
        f.handle(
            "fake-shop.test",
            "POST",
            "/upload",
            "",
            "multipart/form-data; boundary=XYZ",
            body,
        );
        let s = f.seen();
        assert_eq!(s[0]["filename"], "u.pdf");
        assert_eq!(s[0]["size"], 9);
    }
}
