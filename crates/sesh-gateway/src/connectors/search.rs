//! `search` connector: web search through SearXNG, offline encyclopedia
//! search and articles through kiwix-serve (contracts/connectors.md).
//! Request building and response parsing are pure; `mod.rs` does HTTP.

use serde_json::{Value, json};

pub type Query = Vec<(&'static str, String)>;

pub fn web_request(query: &str) -> (&'static str, Query) {
    (
        "/search",
        vec![("q", query.to_string()), ("format", "json".into())],
    )
}

pub fn parse_web(body: &Value, limit: usize) -> Value {
    let results = body["results"].as_array().cloned().unwrap_or_default();
    Value::Array(
        results
            .iter()
            .take(limit)
            .map(|r| {
                json!({
                    "title": r["title"].as_str().unwrap_or(""),
                    "url": r["url"].as_str().unwrap_or(""),
                    "snippet": r["content"].as_str().unwrap_or(""),
                })
            })
            .collect(),
    )
}

pub fn wiki_request(query: &str, limit: usize) -> (&'static str, Query) {
    (
        "/search",
        vec![
            ("pattern", query.to_string()),
            ("format", "xml".into()),
            ("pageLength", limit.to_string()),
        ],
    )
}

fn tag<'a>(block: &'a str, name: &str) -> &'a str {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    block
        .find(&open)
        .and_then(|s| {
            let rest = &block[s + open.len()..];
            rest.find(&close).map(|e| &rest[..e])
        })
        .unwrap_or("")
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Parses kiwix-serve's OpenSearch XML (`<item><title/><link/><description/></item>`).
pub fn parse_wiki(xml: &str, limit: usize) -> Value {
    let mut out = Vec::new();
    for block in xml.split("<item>").skip(1).take(limit) {
        let link = unescape(tag(block, "link"));
        let path = link.trim_start_matches("/content/").to_string();
        out.push(json!({
            "title": unescape(tag(block, "title")),
            "path": path,
            "snippet": strip_tags(&unescape(tag(block, "description"))),
        }));
    }
    Value::Array(out)
}

/// Validates an article path: relative, no traversal, no scheme.
pub fn article_path(path: &str) -> Result<String, String> {
    if path.contains("..") || path.contains("://") || path.starts_with('/') || path.contains('?') {
        return Err("invalid article path".into());
    }
    Ok(format!("/content/{path}"))
}

/// Whether `b` starts with the ASCII `pat`, ignoring ASCII case.
fn starts_with_ci(b: &[u8], pat: &str) -> bool {
    b.get(..pat.len())
        .is_some_and(|h| h.eq_ignore_ascii_case(pat.as_bytes()))
}

/// Strips tags, `<script>` and `<style>` blocks. Safe on any UTF-8: tag
/// names are matched as ASCII bytes, and text is only sliced at char
/// boundaries.
pub fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut skip_until: Option<&str> = None;
    let bytes = html.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(end) = skip_until {
            if starts_with_ci(&bytes[i..], end) {
                i += end.len();
                skip_until = None;
            } else {
                i += 1;
            }
            continue;
        }
        if starts_with_ci(&bytes[i..], "<script") {
            skip_until = Some("</script>");
            continue;
        }
        if starts_with_ci(&bytes[i..], "<style") {
            skip_until = Some("</style>");
            continue;
        }
        // `i` is on a char boundary here (skipping ends after an ASCII
        // pattern); if it ever weren't, stop rather than panic.
        let Some(c) = html.get(i..).and_then(|r| r.chars().next()) else {
            break;
        };
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
        i += c.len_utf8();
    }
    let text = unescape(&out);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parse_article(path: &str, html: &str, cap: usize) -> Value {
    let title = strip_tags(tag(html, "title"));
    let mut text = strip_tags(html);
    if text.len() > cap {
        let mut end = cap;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str(" …");
    }
    json!({"title": if title.is_empty() { path.to_string() } else { title }, "text": text})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_tags_is_safe_on_any_utf8() {
        // Multi-byte characters inside skipped blocks, next to tags, and
        // characters whose lowercase has a different byte length.
        let html = "<p>caf\u{e9}</p><style>p{content:\"\u{e9}\u{2603}\u{1f600}\"}</style>\
                    <SCRIPT>let s='\u{130}\u{e9}';</SCRIPT>\u{130}stanbul <b>\u{2603}</b>";
        assert_eq!(strip_tags(html), "caf\u{e9} \u{130}stanbul \u{2603}");
        // Unclosed blocks and stray brackets never panic.
        assert_eq!(strip_tags("a<style>\u{e9}\u{e9}"), "a");
        assert_eq!(strip_tags("\u{e9}<script\u{e9}"), "\u{e9}");
        assert_eq!(strip_tags("<\u{e9}>x"), "x");
    }

    #[test]
    fn web_parsing_takes_limit_and_maps_fields() {
        let body = json!({"results": [
            {"title": "A", "url": "https://a", "content": "aa"},
            {"title": "B", "url": "https://b", "content": "bb"}]});
        let r = parse_web(&body, 1);
        assert_eq!(
            r,
            json!([{"title": "A", "url": "https://a", "snippet": "aa"}])
        );
        assert_eq!(parse_web(&json!({}), 5), json!([]));
        assert_eq!(web_request("rain").1[0], ("q", "rain".to_string()));
    }

    #[test]
    fn wiki_parsing_reads_items() {
        let xml = "<rss><channel><item><title>Ray &amp; Co</title><link>/content/wiki/A/Ray</link>\
                   <description>&lt;b&gt;Ray&lt;/b&gt; was</description></item></channel></rss>";
        assert_eq!(
            parse_wiki(xml, 5),
            json!([{"title": "Ray & Co", "path": "wiki/A/Ray", "snippet": "Ray was"}])
        );
    }

    #[test]
    fn article_paths_cannot_escape() {
        assert_eq!(article_path("wiki/A/Ray").unwrap(), "/content/wiki/A/Ray");
        for bad in ["../etc", "/abs", "http://x", "a?b=1"] {
            assert!(article_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn article_text_drops_markup_and_scripts() {
        let html = "<html><head><title>Ray</title><script>evil()</script></head><body><p>Hello <b>world</b></p></body></html>";
        let a = parse_article("wiki/A/Ray", html, 1000);
        assert_eq!(a["title"], "Ray");
        assert!(!a["text"].as_str().unwrap().contains("evil"));
        assert!(a["text"].as_str().unwrap().contains("Hello world"));
        let short = parse_article("p", &"x".repeat(100), 10);
        assert!(short["text"].as_str().unwrap().ends_with('…'));
    }
}
