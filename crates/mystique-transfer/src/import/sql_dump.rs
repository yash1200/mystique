//! Best-effort INSERT dump parser for SQL imports.

use serde_json::Value as JsonValue;

use crate::error::{TransferError, TransferResult};

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
    if bytes.get(*i).copied() != Some(b'\'') {
        return None;
    }
    *i += 1;
    let mut out = String::new();
    while *i < s.len() {
        let rest = s.get(*i..)?;
        let ch = rest.chars().next()?;
        if ch == '\'' {
            if bytes.get(*i + 1).copied() == Some(b'\'') {
                out.push('\'');
                *i += 2;
                continue;
            }
            *i += 1;
            return Some(out);
        }
        out.push(ch);
        *i += ch.len_utf8();
    }
    None
}

/// `NULL` / `TRUE` / `FALSE` only when the next char is not still an identifier.
/// A short multibyte tail (`N'日本'`) must not slice inside a character.
fn split_ascii_keyword<'a>(rest: &'a str, keyword: &str) -> Option<&'a str> {
    let head = rest.get(..keyword.len())?;
    if !head.eq_ignore_ascii_case(keyword) {
        return None;
    }
    let after = rest.get(keyword.len()..)?;
    match after.chars().next() {
        Some(ch) if ch.is_ascii_alphanumeric() || ch == '_' => None,
        _ => Some(after),
    }
}

pub(super) fn parse_sql_value(s: &str, i: &mut usize) -> Option<JsonValue> {
    let bytes = s.as_bytes();
    while bytes.get(*i).is_some_and(|b| b.is_ascii_whitespace()) {
        *i += 1;
    }
    let b = bytes.get(*i).copied()?;
    match b {
        b'\'' => parse_sql_string(s, i).map(JsonValue::String),
        b'N' | b'n' => {
            let rest = s.get(*i..)?;
            if split_ascii_keyword(rest, "null").is_some() {
                *i += 4;
                Some(JsonValue::Null)
            } else if bytes.get(*i + 1).copied() == Some(b'\'') {
                // National string literal: N'日本'. The prefix is one ASCII byte.
                *i += 1;
                parse_sql_string(s, i).map(JsonValue::String)
            } else {
                None
            }
        }
        b'T' | b't' => {
            let rest = s.get(*i..)?;
            if split_ascii_keyword(rest, "true").is_some() {
                *i += 4;
                Some(JsonValue::Bool(true))
            } else {
                None
            }
        }
        b'F' | b'f' => {
            let rest = s.get(*i..)?;
            if split_ascii_keyword(rest, "false").is_some() {
                *i += 5;
                Some(JsonValue::Bool(false))
            } else {
                None
            }
        }
        b'"' | b'`' => {
            *i += 1;
            let start = *i;
            while bytes.get(*i).is_some_and(|c| *c != b) {
                *i += 1;
            }
            let text = s.get(start..*i)?.to_string();
            if *i < bytes.len() {
                *i += 1;
            }
            Some(JsonValue::String(text))
        }
        _ => {
            let start = *i;
            while bytes
                .get(*i)
                .is_some_and(|c| !matches!(c, b',' | b')' | b';') && !c.is_ascii_whitespace())
            {
                *i += 1;
            }
            Some(parse_cell(s.get(start..*i)?))
        }
    }
}

pub(super) fn parse_sql_tuple(s: &str, i: &mut usize) -> Option<Vec<JsonValue>> {
    let bytes = s.as_bytes();
    while bytes.get(*i).is_some_and(|b| b.is_ascii_whitespace()) {
        *i += 1;
    }
    if bytes.get(*i).copied() != Some(b'(') {
        return None;
    }
    *i += 1;
    let mut vals = Vec::new();
    loop {
        while bytes.get(*i).is_some_and(|b| b.is_ascii_whitespace()) {
            *i += 1;
        }
        if bytes.get(*i).copied() == Some(b')') {
            *i += 1;
            return Some(vals);
        }
        vals.push(parse_sql_value(s, i)?);
        while bytes.get(*i).is_some_and(|b| b.is_ascii_whitespace()) {
            *i += 1;
        }
        if bytes.get(*i).copied() == Some(b',') {
            *i += 1;
            continue;
        }
        if bytes.get(*i).copied() == Some(b')') {
            *i += 1;
            return Some(vals);
        }
        return None;
    }
}

pub(super) fn parse_sql(text: &str, limit: Option<usize>) -> TransferResult<ParsedTable> {
    let mut keys: Vec<String> = Vec::new();
    let mut rows = Vec::new();
    let lower = text.to_ascii_lowercase();
    let mut search = 0usize;
    while let Some(rel) = lower.get(search..).and_then(|hay| hay.find("insert")) {
        let start = search + rel;
        let Some(after) = text.get(start..) else {
            break;
        };
        let Some(after_l) = lower.get(start..) else {
            break;
        };
        let Some(values_rel) = after_l.find("values") else {
            break;
        };
        let Some(head) = after.get(..values_rel) else {
            break;
        };
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
            while text
                .as_bytes()
                .get(i)
                .is_some_and(|b| b.is_ascii_whitespace())
            {
                i += 1;
            }
            if text.as_bytes().get(i).copied() == Some(b',') {
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
        return Err(TransferError::InvalidConfig(
            "No INSERT rows found in the SQL file.".into(),
        ));
    }
    Ok(ParsedTable { keys, rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn national_string_keeps_utf8() {
        let mut i = 0;
        let value = parse_sql_value("N'日本'", &mut i).expect("value");
        assert_eq!(value, JsonValue::String("日本".into()));
        assert_eq!(i, "N'日本'".len());
    }

    #[test]
    fn insert_of_national_string_does_not_panic() {
        let parsed = parse_sql("INSERT INTO t (name) VALUES (N'日本');", None).expect("rows");
        assert_eq!(parsed.keys, vec!["name".to_string()]);
        assert_eq!(parsed.rows, vec![vec![JsonValue::String("日本".into())]]);
    }

    #[test]
    fn plain_utf8_string_is_not_mojibake() {
        let mut i = 0;
        let value = parse_sql_value("'日本'", &mut i).expect("value");
        assert_eq!(value, JsonValue::String("日本".into()));
    }

    #[test]
    fn null_true_and_false_still_parse() {
        let mut i = 0;
        let row = parse_sql_tuple("(NULL, TRUE, false)", &mut i).expect("tuple");
        assert_eq!(
            row,
            vec![
                JsonValue::Null,
                JsonValue::Bool(true),
                JsonValue::Bool(false),
            ]
        );
    }
}
