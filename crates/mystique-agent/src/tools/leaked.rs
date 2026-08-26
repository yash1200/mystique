use serde_json::{json, Value as JsonValue};

use super::ALLOWED;

/// A tool invocation the model printed as JSON instead of a native `tool_calls` item.
#[derive(Debug, Clone, PartialEq)]
pub struct LeakedToolCall {
    pub name: String,
    pub args: JsonValue,
}

/// Pull `{ "name": "list_objects", "arguments": {…} }` (and close cousins) out of
/// assistant text. Local models — especially Ollama — often dump this instead of
/// using the API's `tool_calls` field, which Rig then treats as the reply.
pub fn extract_leaked_tool_calls(text: &str) -> Vec<LeakedToolCall> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    if let Some(calls) = parse_tool_payload(trimmed) {
        return dedup_calls(calls);
    }
    for fence in fenced_bodies(trimmed) {
        if let Some(calls) = parse_tool_payload(fence) {
            out.extend(calls);
        }
    }
    for inner in xml_tag_bodies(trimmed, "tool_call") {
        if let Some(calls) = parse_tool_payload(inner) {
            out.extend(calls);
        }
    }
    if out.is_empty() {
        for obj in scan_json_values(trimmed) {
            if let Some(calls) = parse_tool_value(&obj) {
                out.extend(calls);
            }
        }
    }
    dedup_calls(out)
}

/// Remove leaked tool-call JSON so it is not shown as the assistant's answer.
pub fn strip_leaked_tool_calls(text: &str) -> String {
    let calls = extract_leaked_tool_calls(text);
    if calls.is_empty() {
        return text.to_string();
    }
    let mut kept = text.to_string();
    for raw in scan_json_slices(text) {
        if serde_json::from_str::<JsonValue>(raw)
            .ok()
            .and_then(|v| parse_tool_value(&v))
            .is_some()
        {
            kept = kept.replacen(raw, "", 1);
        }
    }
    for fence in fenced_blocks(text) {
        if parse_tool_payload(fence.body).is_some() {
            kept = kept.replacen(fence.full, "", 1);
        }
    }
    collapse_blank_lines(&kept)
}

fn parse_tool_payload(s: &str) -> Option<Vec<LeakedToolCall>> {
    parse_tool_value(&serde_json::from_str(s).ok()?)
}

fn parse_tool_value(v: &JsonValue) -> Option<Vec<LeakedToolCall>> {
    match v {
        JsonValue::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                if let Some(calls) = parse_tool_value(item) {
                    out.extend(calls);
                }
            }
            if out.is_empty() {
                None
            } else {
                Some(out)
            }
        }
        JsonValue::Object(map) => {
            if let Some(func) = map.get("function") {
                return parse_tool_value(func);
            }
            let name = map
                .get("name")
                .or_else(|| map.get("tool"))
                .and_then(|n| n.as_str())?;
            if !ALLOWED.contains(&name) {
                return None;
            }
            let args = map
                .get("arguments")
                .or_else(|| map.get("parameters"))
                .or_else(|| map.get("args"))
                .cloned()
                .unwrap_or_else(|| json!({}));
            let args = if args.is_object() || args.is_null() {
                if args.is_null() {
                    json!({})
                } else {
                    args
                }
            } else {
                json!({})
            };
            Some(vec![LeakedToolCall {
                name: name.to_string(),
                args,
            }])
        }
        _ => None,
    }
}

fn fenced_bodies(text: &str) -> Vec<&str> {
    fenced_blocks(text).into_iter().map(|b| b.body).collect()
}

struct Fence<'a> {
    full: &'a str,
    body: &'a str,
}

fn fenced_blocks(text: &str) -> Vec<Fence<'_>> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let mut ticks = 0;
            while i + ticks < bytes.len() && bytes[i + ticks] == b'`' {
                ticks += 1;
            }
            if ticks >= 3 {
                let open = i;
                i += ticks;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
                let body_start = i;
                let marker = &text[open..open + ticks];
                if let Some(rel) = text[i..].find(marker) {
                    let body_end = i + rel;
                    let full_end = body_end + ticks;
                    out.push(Fence {
                        full: &text[open..full_end],
                        body: text[body_start..body_end].trim(),
                    });
                    i = full_end;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

fn xml_tag_bodies<'a>(text: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start + open.len()..];
        if let Some(end) = after.find(&close) {
            out.push(after[..end].trim());
            rest = &after[end + close.len()..];
        } else {
            break;
        }
    }
    out
}

fn scan_json_values(text: &str) -> Vec<JsonValue> {
    scan_json_slices(text)
        .into_iter()
        .filter_map(|s| serde_json::from_str(s).ok())
        .collect()
}

fn scan_json_slices(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' || bytes[i] == b'[' {
            if let Some(end) = matching_json_end(text, i) {
                out.push(&text[i..=end]);
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn matching_json_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let open = bytes[start];
    let close = if open == b'{' { b'}' } else { b']' };
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, &c) in bytes[start..].iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
            continue;
        }
        match c {
            b'"' => in_str = true,
            b if b == open => depth += 1,
            b if b == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + i);
                }
            }
            _ => {}
        }
    }
    None
}

fn dedup_calls(calls: Vec<LeakedToolCall>) -> Vec<LeakedToolCall> {
    let mut out = Vec::new();
    for call in calls {
        if out.last() != Some(&call) {
            out.push(call);
        }
    }
    out
}

fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for line in text.lines() {
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::{extract_leaked_tool_calls, strip_leaked_tool_calls};

    #[test]
    fn extracts_ollama_style_name_arguments() {
        let text = r#"{"name": "list_objects", "arguments": {"database": "qna_common_db"}}"#;
        let calls = extract_leaked_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "list_objects");
        assert_eq!(calls[0].args["database"], "qna_common_db");
    }

    #[test]
    fn extracts_empty_arguments_object() {
        let text = r#"{"name": "list_saved_queries", "arguments": {}}"#;
        let calls = extract_leaked_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "list_saved_queries");
    }

    #[test]
    fn extracts_tool_json_mixed_with_prose() {
        let text = "Hi\n{\"name\": \"list_objects\", \"arguments\": {\"database\": \"qna_common_db\"}}\nCan you write a query?\n{\"name\": \"list_saved_queries\", \"arguments\": {}}";
        let calls = extract_leaked_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "list_objects");
        assert_eq!(calls[1].name, "list_saved_queries");
    }

    #[test]
    fn ignores_unrelated_json() {
        let text = r#"{"name": "users", "columns": ["id"]}"#;
        assert!(extract_leaked_tool_calls(text).is_empty());
    }

    #[test]
    fn strips_leaked_json_leaving_prose() {
        let text = "Hi\n{\"name\": \"list_objects\", \"arguments\": {\"database\": \"qna_common_db\"}}\nCan you write a query?";
        let cleaned = strip_leaked_tool_calls(text);
        assert!(cleaned.contains("Hi"));
        assert!(cleaned.contains("Can you write a query?"));
        assert!(!cleaned.contains("list_objects"));
    }
}
