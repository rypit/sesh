//! Input validation for the JSON Schema subset sesh's connectors declare,
//! and TypeScript declarations for `describe` (research.md R7).
//!
//! Supported: `type` object (properties, required, no extra keys), string
//! (minLength, maxLength), integer and number (minimum, maximum), boolean,
//! array (items, maxItems), and `enum` of strings.

use serde_json::Value;

/// Validates `value` against `schema`; returns the first problem found.
pub fn validate(schema: &Value, value: &Value) -> Result<(), String> {
    check(schema, value, "input")
}

fn check(schema: &Value, value: &Value, at: &str) -> Result<(), String> {
    if let Some(options) = schema.get("enum").and_then(Value::as_array) {
        return if options.contains(value) {
            Ok(())
        } else {
            Err(format!(
                "{at} must be one of {}",
                Value::Array(options.clone())
            ))
        };
    }
    let ty = schema
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("object");
    let num = |k: &str| schema.get(k).and_then(Value::as_f64);
    match ty {
        "object" => {
            let Some(obj) = value.as_object() else {
                return Err(format!("{at} must be an object"));
            };
            let props = schema.get("properties").and_then(Value::as_object);
            if let Some(required) = schema.get("required").and_then(Value::as_array) {
                for r in required.iter().filter_map(Value::as_str) {
                    if !obj.contains_key(r) {
                        return Err(format!("{at}.{r} is required"));
                    }
                }
            }
            for (k, v) in obj {
                match props.and_then(|p| p.get(k)) {
                    Some(s) => check(s, v, &format!("{at}.{k}"))?,
                    None => return Err(format!("{at}.{k} is not allowed")),
                }
            }
            Ok(())
        }
        "string" => {
            let Some(s) = value.as_str() else {
                return Err(format!("{at} must be a string"));
            };
            let len = s.chars().count() as f64;
            if num("minLength").is_some_and(|m| len < m) {
                return Err(format!("{at} is too short"));
            }
            if num("maxLength").is_some_and(|m| len > m) {
                return Err(format!("{at} is too long"));
            }
            Ok(())
        }
        "integer" | "number" => {
            let Some(n) = value.as_f64() else {
                return Err(format!("{at} must be a {ty}"));
            };
            if ty == "integer" && n.fract() != 0.0 {
                return Err(format!("{at} must be an integer"));
            }
            if num("minimum").is_some_and(|m| n < m) || num("maximum").is_some_and(|m| n > m) {
                return Err(format!("{at} is out of range"));
            }
            Ok(())
        }
        "boolean" => value
            .is_boolean()
            .then_some(())
            .ok_or_else(|| format!("{at} must be true or false")),
        "array" => {
            let Some(items) = value.as_array() else {
                return Err(format!("{at} must be an array"));
            };
            if num("maxItems").is_some_and(|m| items.len() as f64 > m) {
                return Err(format!("{at} has too many items"));
            }
            if let Some(item_schema) = schema.get("items") {
                for (i, item) in items.iter().enumerate() {
                    check(item_schema, item, &format!("{at}[{i}]"))?;
                }
            }
            Ok(())
        }
        other => Err(format!("{at}: unsupported schema type {other}")),
    }
}

/// A TypeScript type for `schema`.
pub fn typescript(schema: &Value) -> String {
    if let Some(options) = schema.get("enum").and_then(Value::as_array) {
        return options
            .iter()
            .map(|o| o.to_string())
            .collect::<Vec<_>>()
            .join(" | ");
    }
    match schema
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("object")
    {
        "string" => "string".into(),
        "integer" | "number" => "number".into(),
        "boolean" => "boolean".into(),
        "array" => {
            let inner = schema
                .get("items")
                .map(typescript)
                .unwrap_or_else(|| "unknown".into());
            format!("Array<{inner}>")
        }
        "object" => {
            let required: Vec<&str> = schema
                .get("required")
                .and_then(Value::as_array)
                .map(|r| r.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let Some(props) = schema.get("properties").and_then(Value::as_object) else {
                return "Record<string, never>".into();
            };
            let fields: Vec<String> = props
                .iter()
                .map(|(k, v)| {
                    let opt = if required.contains(&k.as_str()) {
                        ""
                    } else {
                        "?"
                    };
                    format!("{k}{opt}: {}", typescript(v))
                })
                .collect();
            format!("{{ {} }}", fields.join("; "))
        }
        _ => "unknown".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({"type": "object", "required": ["query"], "properties": {
            "query": {"type": "string", "minLength": 1, "maxLength": 5},
            "limit": {"type": "integer", "minimum": 1, "maximum": 20},
            "units": {"enum": ["c", "f"]},
            "tags": {"type": "array", "maxItems": 2, "items": {"type": "string"}}
        }})
    }

    #[test]
    fn accepts_valid_input() {
        validate(
            &schema(),
            &json!({"query": "rain", "limit": 3, "units": "c", "tags": ["a"]}),
        )
        .unwrap();
    }

    #[test]
    fn rejects_each_kind_of_bad_input() {
        for (bad, why) in [
            (json!({}), "required"),
            (json!({"query": ""}), "too short"),
            (json!({"query": "toolong"}), "too long"),
            (json!({"query": "a", "limit": 2.5}), "integer"),
            (json!({"query": "a", "limit": 99}), "range"),
            (json!({"query": "a", "units": "k"}), "one of"),
            (json!({"query": "a", "tags": ["a", "b", "c"]}), "too many"),
            (json!({"query": "a", "tags": [1]}), "string"),
            (json!({"query": "a", "user": "bob"}), "not allowed"),
            (json!("query"), "object"),
        ] {
            let err = validate(&schema(), &bad).unwrap_err();
            assert!(err.contains(why), "{bad}: {err}");
        }
    }

    #[test]
    fn generates_typescript() {
        assert_eq!(
            typescript(&schema()),
            r#"{ limit?: number; query: string; tags?: Array<string>; units?: "c" | "f" }"#
        );
        assert_eq!(
            typescript(&json!({"type": "object"})),
            "Record<string, never>"
        );
    }
}
