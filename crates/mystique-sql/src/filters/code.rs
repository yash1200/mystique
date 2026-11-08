//! Parse the grid code-filter box (JSON object or `col = val` predicates).

use super::TableFilter;
use crate::error::{BuildError, BuildResult};

/// Parse the code-filter box: JSON `{ "col": "val" }` or `col = val AND …`.
pub fn parse_code_filter_rules(code: &str) -> BuildResult<Vec<TableFilter>> {
    let t = code.trim();
    if t.is_empty() {
        return Ok(Vec::new());
    }
    if t.starts_with('{') {
        return object_to_eq_filters(t);
    }
    parse_predicate_lines(t)
}

fn object_to_eq_filters(raw: &str) -> BuildResult<Vec<TableFilter>> {
    let v = parse_loose_json_object(raw)?;
    let obj = v.as_object().ok_or_else(|| {
        BuildError::new("code filter JSON must be an object `{ column: value }`".into())
    })?;
    let mut out = Vec::with_capacity(obj.len());
    for (k, val) in obj {
        let value = match val {
            serde_json::Value::Null => "null".into(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            _ => {
                return Err(BuildError::new(format!(
                    "JSON filter value for `{k}` must be a string, number, boolean, or null"
                )));
            }
        };
        out.push(TableFilter {
            column: k.clone(),
            op: "eq".into(),
            value,
        });
    }
    Ok(out)
}

fn parse_loose_json_object(raw: &str) -> BuildResult<serde_json::Value> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        return Ok(v);
    }
    let quoted = quote_unquoted_json_keys(raw);
    serde_json::from_str(&quoted)
        .map_err(|e| BuildError::new(format!("invalid code filter JSON: {e}")))
}

fn quote_unquoted_json_keys(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len() + 16);
    let mut i = 0;
    let mut in_str: Option<char> = None;
    let mut esc = false;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            out.push(c);
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' {
            in_str = Some(c);
            out.push(if c == '\'' { '"' } else { c });
            i += 1;
            continue;
        }
        if c == '_' || c == '$' || c.is_ascii_alphabetic() {
            let start = i;
            i += 1;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '$')
            {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();
            let mut j = i;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && chars[j] == ':' {
                out.push('"');
                out.push_str(&ident);
                out.push('"');
                out.extend(chars[i..j].iter());
                out.push(':');
                i = j + 1;
                continue;
            }
            out.push_str(&ident);
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn parse_predicate_lines(code: &str) -> BuildResult<Vec<TableFilter>> {
    let chunks: Vec<&str> = code
        .split(|c| c == '\n')
        .flat_map(|line| line.split(" AND ").flat_map(|p| p.split(" and ")))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if chunks.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(chunks.len());
    for line in chunks {
        let (column, op, value) = parse_one_predicate(line)?;
        out.push(TableFilter { column, op, value });
    }
    Ok(out)
}

fn parse_one_predicate(line: &str) -> BuildResult<(String, String, String)> {
    let lower = line.to_ascii_lowercase();
    let named: &[(&str, &str)] = &[
        (" is not empty", "not_empty"),
        (" is empty", "is_empty"),
        (" is not null", "not_null"),
        (" is null", "is_null"),
        (" starts with ", "starts"),
        (" ends with ", "ends"),
        (" contains ", "contains"),
    ];
    for (needle, op) in named {
        if let Some(idx) = lower.find(needle) {
            let column = line[..idx].trim().to_string();
            if column.is_empty() {
                break;
            }
            let value = if *op == "starts" || *op == "ends" || *op == "contains" {
                unquote_filter_value(line[idx + needle.len()..].trim())
            } else {
                String::new()
            };
            return Ok((column, (*op).into(), value));
        }
    }
    let ops = [
        (">=", "gte"),
        ("<=", "lte"),
        ("!=", "neq"),
        ("<>", "neq"),
        ("=", "eq"),
        (">", "gt"),
        ("<", "lt"),
    ];
    for (sym, op) in ops {
        if let Some(idx) = line.find(sym) {
            let column = line[..idx].trim().to_string();
            if column.is_empty() {
                continue;
            }
            let value = unquote_filter_value(line[idx + sym.len()..].trim());
            return Ok((column, op.into(), value));
        }
    }
    Err(BuildError::new(format!(
        "could not parse filter `{line}` — use {{ \"column\": \"value\" }} or column = value"
    )))
}

fn unquote_filter_value(raw: &str) -> String {
    let t = raw.trim();
    if (t.starts_with('\'') && t.ends_with('\'')) || (t.starts_with('"') && t.ends_with('"')) {
        return t[1..t.len() - 1].to_string();
    }
    t.to_string()
}
