//! Best-effort INSERT dump parser for SQL imports.

use serde_json::Value as JsonValue;

use crate::error::{ConnectorError, ConnectorResult};

use super::parse::{pad_row, parse_cell, ParsedTable};

pub(super) fn strip_ident(raw: &str) -> String {
    raw.trim()
        .trim_matches(|c| c == '"' || c == '`' || c == '[' || c == ']')
        .to_string()
}

pub(super) fn parse_ident_list(inner: &str) -> Vec<String> {
    inner
        .split(',')
        .map(strip_ident)
        .filter(|s| !s.is_empty())
        .collect()
}

pub(super) fn parse_sql_string(s: &str, i: &mut usize) -> Option<String> {
    let bytes = s.as_bytes();
    if *i >= bytes.len() || bytes[*i] != b'\'' {
        return None;
    }
    *i += 1;
    let mut out = String::new();
    while *i < bytes.len() {
        if bytes[*i] == b'\'' {
            if *i + 1 < bytes.len() && bytes[*i + 1] == b'\'' {
                out.push('\'');
                *i += 2;
                continue;
            }
            *i += 1;
            return Some(out);
        }
        out.push(bytes[*i] as char);
        *i += 1;
    }
    None
}

pub(super) fn parse_sql_value(s: &str, i: &mut usize) -> Option<JsonValue> {
    let bytes = s.as_bytes();
    while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
        *i += 1;
    }
    if *i >= bytes.len() {
        return None;
    }
    match bytes[*i] {
        b'\'' => parse_sql_string(s, i).map(JsonValue::String),
        b'N' | b'n' => {
            let rest = &s[*i..];
            if rest.len() >= 4 && rest[..4].eq_ignore_ascii_case("null") {
                *i += 4;
                Some(JsonValue::Null)
            } else {
                None
            }
        }
        b'T' | b't' => {
            let rest = &s[*i..];
            if rest.len() >= 4 && rest[..4].eq_ignore_ascii_case("true") {
                *i += 4;
                Some(JsonValue::Bool(true))
            } else {
                None
            }
        }
        b'F' | b'f' => {
            let rest = &s[*i..];
            if rest.len() >= 5 && rest[..5].eq_ignore_ascii_case("false") {
                *i += 5;
                Some(JsonValue::Bool(false))
            } else {
                None
            }
        }
        b'"' | b'`' => {
            let q = bytes[*i];
            *i += 1;
            let start = *i;
            while *i < bytes.len() && bytes[*i] != q {
                *i += 1;
            }
            let s = s[start..*i].to_string();
            if *i < bytes.len() {
                *i += 1;
            }
            Some(JsonValue::String(s))
        }
        _ => {
            let start = *i;
            while *i < bytes.len()
                && !matches!(bytes[*i], b',' | b')' | b';')
                && !bytes[*i].is_ascii_whitespace()
            {
                *i += 1;
            }
            Some(parse_cell(&s[start..*i]))
        }
    }
}

pub(super) fn parse_sql_tuple(s: &str, i: &mut usize) -> Option<Vec<JsonValue>> {
    let bytes = s.as_bytes();
    while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
        *i += 1;
    }
    if *i >= bytes.len() || bytes[*i] != b'(' {
        return None;
    }
    *i += 1;
    let mut vals = Vec::new();
    loop {
        while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
            *i += 1;
        }
        if *i < bytes.len() && bytes[*i] == b')' {
            *i += 1;
            return Some(vals);
        }
        vals.push(parse_sql_value(s, i)?);
        while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
            *i += 1;
        }
        if *i < bytes.len() && bytes[*i] == b',' {
            *i += 1;
            continue;
        }
        if *i < bytes.len() && bytes[*i] == b')' {
            *i += 1;
            return Some(vals);
        }
        return None;
    }
}

pub(super) fn parse_sql(text: &str, limit: Option<usize>) -> ConnectorResult<ParsedTable> {
    let mut keys: Vec<String> = Vec::new();
    let mut rows = Vec::new();
    let lower = text.to_ascii_lowercase();
    let mut search = 0usize;
    while let Some(rel) = lower[search..].find("insert") {
        let start = search + rel;
        let after = &text[start..];
        let after_l = &lower[start..];
        let Some(values_rel) = after_l.find("values") else {
            break;
        };
        let head = &after[..values_rel];
        let mut col_keys = Vec::new();
        if let (Some(open), Some(close)) = (head.rfind('('), head.rfind(')')) {
            if open < close {
                col_keys = parse_ident_list(&head[open + 1..close]);
            }
        }
        let mut i = start + values_rel + "values".len();
        while let Some(tuple) = parse_sql_tuple(text, &mut i) {
            if keys.is_empty() {
                keys = if col_keys.is_empty() {
                    (0..tuple.len()).map(|n| format!("col_{}", n + 1)).collect()
                } else {
                    col_keys.clone()
                };
            }
            rows.push(pad_row(tuple, keys.len()));
            if limit.is_some_and(|n| rows.len() >= n) {
                return Ok(ParsedTable { keys, rows });
            }
            while i < text.len() && text.as_bytes()[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < text.len() && text.as_bytes()[i] == b',' {
                i += 1;
                continue;
            }
            break;
        }
        search = i.max(start + 6);
        if search >= text.len() {
            break;
        }
    }
    if keys.is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "No INSERT rows found in the SQL file.".into(),
        ));
    }
    Ok(ParsedTable { keys, rows })
}
