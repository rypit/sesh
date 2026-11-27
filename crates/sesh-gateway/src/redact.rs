//! Argument summaries for the audit log (data-model.md, Audit entry
//! `args_summary`): keys and value types, strings cut to 40 characters,
//! nothing nested deeper than three levels.

use serde_json::{Map, Value, json};

const MAX_STRING: usize = 40;
const MAX_DEPTH: usize = 3;
const MAX_ITEMS: usize = 5;

pub fn summarize(args: &Value) -> Value {
    summary(args, 0)
}

fn summary(v: &Value, depth: usize) -> Value {
    if depth >= MAX_DEPTH {
        return json!("…");
    }
    match v {
        Value::String(s) => {
            let cut: String = s.chars().take(MAX_STRING).collect();
            if cut.len() < s.len() {
                Value::String(format!("{cut}… ({} chars)", s.chars().count()))
            } else {
                Value::String(cut)
            }
        }
        Value::Array(items) => {
            let mut out: Vec<Value> = items
                .iter()
                .take(MAX_ITEMS)
                .map(|x| summary(x, depth + 1))
                .collect();
            if items.len() > MAX_ITEMS {
                out.push(json!(format!("… {} more", items.len() - MAX_ITEMS)));
            }
            Value::Array(out)
        }
        Value::Object(o) => {
            let mut out = Map::new();
            for (k, x) in o {
                out.insert(k.clone(), summary(x, depth + 1));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_long_strings_and_lists() {
        let long = "x".repeat(100);
        let s = summarize(&json!({"q": long, "n": 3, "list": [1, 2, 3, 4, 5, 6, 7]}));
        assert_eq!(s["n"], 3);
        assert!(s["q"].as_str().unwrap().ends_with("(100 chars)"));
        assert_eq!(s["list"].as_array().unwrap().len(), 6);
    }

    #[test]
    fn limits_depth() {
        let s = summarize(&json!({"a": {"b": {"c": {"d": 1}}}}));
        assert_eq!(s["a"]["b"]["c"], "…");
    }
}
