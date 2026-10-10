use chrono::{DateTime, Local};
use serde_json::{json, Value};

pub(crate) fn format_local_time(utc_rfc3339: &str) -> String {
    DateTime::parse_from_rfc3339(utc_rfc3339)
        .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|_| utc_rfc3339.to_string())
}

pub(crate) const MAX_RESPONSE_BYTES: usize = 800_000;

pub(crate) fn text_content(text: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }]
    })
}

pub(crate) fn json_text(v: &Value) -> Value {
    let pretty = serde_json::to_string_pretty(v).unwrap_or_default();
    if pretty.len() <= MAX_RESPONSE_BYTES {
        return text_content(&pretty);
    }
    truncate_response(v)
}

pub(crate) fn truncate_response(v: &Value) -> Value {
    let obj = match v.as_object() {
        Some(o) => o,
        None => {
            let s = serde_json::to_string_pretty(v).unwrap_or_default();
            let truncated = &s[..floor_char_boundary(&s, MAX_RESPONSE_BYTES)];
            return text_content(&format!("{truncated}\n\n⚠ Response truncated. Use limit/offset parameters to paginate."));
        }
    };

    let mut shrunk = serde_json::Map::new();
    for (key, val) in obj {
        if let Some(arr) = val.as_array() {
            if serde_json::to_string(val).unwrap_or_default().len() > MAX_RESPONSE_BYTES / 2 {
                let take = (arr.len() / 4).max(10).min(arr.len());
                let truncated: Vec<Value> = arr.iter().take(take).cloned().collect();
                shrunk.insert(key.clone(), json!(truncated));
                shrunk.insert(
                    format!("{key}_pagination"),
                    json!({
                        "shown": take,
                        "total": arr.len(),
                        "next_offset": take,
                        "hint": format!("Use offset={} and limit={} to fetch the next page", take, take)
                    }),
                );
                continue;
            }
        }
        shrunk.insert(key.clone(), val.clone());
    }

    let result = Value::Object(shrunk);
    let pretty = serde_json::to_string_pretty(&result).unwrap_or_default();
    if pretty.len() > MAX_RESPONSE_BYTES {
        let truncated = &pretty[..floor_char_boundary(&pretty, MAX_RESPONSE_BYTES)];
        text_content(&format!("{truncated}\n\n⚠ Response truncated. Use limit/offset parameters to paginate."))
    } else {
        text_content(&pretty)
    }
}

/// The largest char boundary of `s` at or below `index` (capped at `s.len()`), so slicing there
/// can't panic inside a multi-byte character. Hand-rolled `str::floor_char_boundary`, which needs
/// Rust 1.91.
pub(crate) fn floor_char_boundary(s: &str, index: usize) -> usize {
    let mut end = index.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    end
}

pub(crate) fn err_text(msg: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": msg }],
        "isError": true
    })
}

pub(crate) fn compact_symbol_line(s: &Value) -> String {
    let name = s.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let kind = s.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
    let file = s.get("file").and_then(|v| v.as_str()).unwrap_or("?");
    let line = s.get("line_start").and_then(|v| v.as_i64()).unwrap_or(0);
    let sig = s.get("signature").and_then(|v| v.as_str()).unwrap_or("");
    if sig.is_empty() {
        format!("  {name} [{kind}] {file}:{line}")
    } else {
        format!("  {name} [{kind}] {file}:{line}\n    ({sig})")
    }
}

pub(crate) fn compact_edge_line(e: &Value) -> String {
    let name = e.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let kind = e.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
    let file = e.get("file").and_then(|v| v.as_str()).unwrap_or("?");
    let line = e.get("line_start").and_then(|v| v.as_i64()).unwrap_or(0);
    let sig = e.get("signature").and_then(|v| v.as_str()).unwrap_or("");
    if sig.is_empty() {
        format!("  {name} [{kind}] {file}:{line}")
    } else {
        format!("  {name} [{kind}] {file}:{line}\n    ({sig})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(v: &Value) -> &str {
        v["content"][0]["text"].as_str().unwrap()
    }

    #[test]
    fn truncate_response_cuts_on_a_char_boundary() {
        // Pretty JSON of a string starts with `"`, so every 2-byte `é` starts on an odd byte and
        // MAX_RESPONSE_BYTES (even) falls inside one.
        let long = "é".repeat(MAX_RESPONSE_BYTES);
        let out = truncate_response(&json!(long));
        assert!(text(&out).starts_with('"'));
        assert!(text(&out).contains("Response truncated"));

        // An object whose only field is a huge string isn't shrunk, so it's hard-cut too.
        let out = truncate_response(&json!({ "a": long }));
        assert!(text(&out).contains("Response truncated"));
        assert!(text(&out).len() < MAX_RESPONSE_BYTES + 100);
    }
}
