//! The embedding service client (specs/011 research.md R4): text in,
//! 384-dimensional vectors out, from text-embeddings-inference's
//! OpenAI-style `/v1/embeddings` (or the stub's, in stub mode).

use serde_json::{Value, json};
use std::time::Duration;

pub const DIMENSIONS: usize = 384;

/// E5 models are trained with these prefixes: questions as `query: `,
/// stored text as `passage: `.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Query,
    Passage,
}

pub fn prefixed(kind: Kind, text: &str) -> String {
    match kind {
        Kind::Query => format!("query: {text}"),
        Kind::Passage => format!("passage: {text}"),
    }
}

#[derive(Clone)]
pub struct Embedder {
    url: String,
    http: reqwest::Client,
}

impl Embedder {
    pub fn new(url: &str) -> Self {
        Embedder {
            url: url.trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .no_proxy()
                .build()
                .expect("http client"),
        }
    }

    /// The vector for `text`, normalised; an error if the service is down
    /// or answers with the wrong size.
    pub async fn embed(&self, kind: Kind, text: &str) -> Result<Vec<f32>, String> {
        let r = self
            .http
            .post(format!("{}/v1/embeddings", self.url))
            .json(&json!({"input": [prefixed(kind, text)], "model": "embed"}))
            .send()
            .await
            .map_err(|_| "the embedding service is unavailable".to_string())?;
        if !r.status().is_success() {
            return Err(format!("the embedding service answered {}", r.status()));
        }
        let v: Value = r
            .json()
            .await
            .map_err(|_| "the embedding service sent an unreadable reply".to_string())?;
        parse(&v)
    }

    pub async fn healthy(&self) -> bool {
        self.embed(Kind::Query, "ok").await.is_ok()
    }
}

/// The first vector of an OpenAI-style embeddings reply, normalised.
pub fn parse(v: &Value) -> Result<Vec<f32>, String> {
    let e: Vec<f32> = v["data"][0]["embedding"]
        .as_array()
        .ok_or("no embedding in the reply")?
        .iter()
        .map(|x| x.as_f64().map(|f| f as f32))
        .collect::<Option<_>>()
        .ok_or("an embedding value isn't a number")?;
    if e.len() != DIMENSIONS {
        return Err(format!(
            "the embedding has {} dimensions, not {DIMENSIONS}",
            e.len()
        ));
    }
    Ok(normalise(e))
}

pub fn normalise(mut v: Vec<f32>) -> Vec<f32> {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        v.iter_mut().for_each(|x| *x /= n);
    }
    v
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// A vector as a pgvector literal (`'[0.1,0.2]'::vector`).
pub fn literal(v: &[f32]) -> String {
    let parts: Vec<String> = v.iter().map(|x| format!("{x}")).collect();
    format!("[{}]", parts.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_are_checked_and_normalised() {
        let mut e = vec![0.0f64; DIMENSIONS];
        e[0] = 3.0;
        e[1] = 4.0;
        let v = parse(&json!({"data": [{"embedding": e}]})).unwrap();
        assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
        assert!((cosine(&v, &v) - 1.0).abs() < 1e-6);
        assert!(
            parse(&json!({"data": [{"embedding": [1.0, 2.0]}]}))
                .unwrap_err()
                .contains("dimensions")
        );
        assert!(parse(&json!({"data": []})).is_err());
        assert!(parse(&json!({"data": [{"embedding": ["x"]}]})).is_err());
    }

    #[test]
    fn e5_prefixes_and_pgvector_literals() {
        assert_eq!(prefixed(Kind::Query, "x"), "query: x");
        assert_eq!(prefixed(Kind::Passage, "x"), "passage: x");
        assert_eq!(literal(&[0.5, -1.0]), "[0.5,-1]");
    }

    #[tokio::test]
    async fn a_missing_service_is_an_error_not_a_hang() {
        let e = Embedder::new("http://127.0.0.1:9");
        assert!(e.embed(Kind::Query, "x").await.is_err());
        assert!(!e.healthy().await);
    }
}
