//! Parse `db.coll.find(filter, projection).sort(...).limit(...)` shell reads.

use mongodb::bson::Document;
use mystique_connector_api::{ConnectorError, ConnectorResult};

use crate::shell_filter::parse_filter_object;

#[derive(Debug, Clone, Default)]
pub struct ParsedFind {
    pub collection: Option<String>,
    pub filter: Document,
    pub projection: Option<Document>,
    pub sort: Option<Document>,
    pub limit: Option<i64>,
    pub skip: Option<u64>,
}

/// Bare `{ … }` filter, or a shell `find(...)` with optional projection / sort / limit / skip.
pub fn parse_read_query(text: &str) -> ConnectorResult<ParsedFind> {
    let t = text.trim();
    if t.is_empty() {
        return Err(ConnectorError::Query("Query is empty.".into()));
    }
    if t.starts_with('{') {
        let obj = slice_balanced(t, 0, '{', '}')?;
        let rest = t
            .get(obj.len()..)
            .unwrap_or("")
            .trim()
            .trim_end_matches(';')
            .trim();
        if !rest.is_empty() {
            return Err(ConnectorError::Query(format!(
                "unexpected text after filter: {rest}"
            )));
        }
        return Ok(ParsedFind {
            filter: parse_object_or_empty(obj)?,
            ..ParsedFind::default()
        });
    }

    let lower = t.to_ascii_lowercase();
    let find_at = find_ident(&lower, "find").ok_or_else(|| {
        ConnectorError::Query(
            "Mongo query must be a JSON filter `{ ... }` or `db.collection.find(...)`.".into(),
        )
    })?;

    let collection = collection_before_find(t, find_at);
    let after_name = skip_ws(t, find_at + 4);
    if t.as_bytes().get(after_name) != Some(&b'(') {
        return Ok(ParsedFind {
            collection,
            filter: Document::new(),
            ..ParsedFind::default()
        });
    }

    let call = slice_balanced(t, after_name, '(', ')')?;
    let inner = peel(call);
    let args = split_top_level_objects(inner)?;

    let filter = match args.first() {
        Some(arg) => parse_object_or_empty(arg)?,
        None => Document::new(),
    };
    let projection = match args.get(1) {
        Some(arg) => Some(parse_filter_object(arg)?),
        None => None,
    };

    let mut parsed = ParsedFind {
        collection,
        filter,
        projection,
        ..ParsedFind::default()
    };

    let mut rest_i = after_name + call.len();
    loop {
        rest_i = skip_ws(t, rest_i);
        if rest_i >= t.len() {
            break;
        }
        if t.as_bytes().get(rest_i) == Some(&b';') {
            rest_i += 1;
            continue;
        }
        if t.as_bytes().get(rest_i) != Some(&b'.') {
            let rest = t
                .get(rest_i..)
                .unwrap_or("")
                .trim()
                .trim_end_matches(';')
                .trim();
            if rest.is_empty() {
                break;
            }
            return Err(ConnectorError::Query(format!(
                "unexpected text after find(…): {rest}"
            )));
        }
        rest_i += 1;
        rest_i = skip_ws(t, rest_i);
        let name_end = ident_end(t, rest_i);
        if name_end == rest_i {
            return Err(ConnectorError::Query("expected method after `.`".into()));
        }
        let method = t.get(rest_i..name_end).unwrap_or("").to_ascii_lowercase();
        rest_i = skip_ws(t, name_end);
        if t.as_bytes().get(rest_i) != Some(&b'(') {
            return Err(ConnectorError::Query(format!(
                "expected `(` after .{method}"
            )));
        }
        let args_call = slice_balanced(t, rest_i, '(', ')')?;
        let args_inner = peel(args_call).trim();
        rest_i += args_call.len();

        match method.as_str() {
            "sort" => {
                let obj = first_object_arg(args_inner)?;
                parsed.sort = Some(parse_filter_object(obj)?);
            }
            "project" | "projection" => {
                let obj = first_object_arg(args_inner)?;
                parsed.projection = Some(parse_filter_object(obj)?);
            }
            "limit" => {
                parsed.limit = Some(parse_i64_arg(args_inner, "limit")?);
            }
            "skip" => {
                let n = parse_i64_arg(args_inner, "skip")?;
                parsed.skip = Some(n.max(0) as u64);
            }
            "pretty" | "toarray" | "explain" => {}
            other => {
                return Err(ConnectorError::Query(format!(
                    "unsupported chain `.{other}()` — use .sort / .limit / .skip / .project"
                )));
            }
        }
    }

    Ok(parsed)
}

fn parse_object_or_empty(slice: &str) -> ConnectorResult<Document> {
    let compact: String = slice.chars().filter(|c| !c.is_whitespace()).collect();
    if compact == "{}" {
        return Ok(Document::new());
    }
    parse_filter_object(slice)
}

fn first_object_arg(inner: &str) -> ConnectorResult<&str> {
    let t = inner.trim();
    if t.is_empty() {
        return Err(ConnectorError::Query("expected object argument".into()));
    }
    if !t.starts_with('{') {
        return Err(ConnectorError::Query(format!(
            "expected object `{{ … }}`, got `{t}`"
        )));
    }
    slice_balanced(t, 0, '{', '}')
}

fn parse_i64_arg(inner: &str, name: &str) -> ConnectorResult<i64> {
    let t = inner.trim().trim_end_matches(',').trim();
    t.parse::<i64>()
        .map_err(|_| ConnectorError::Query(format!(".{name}() needs an integer, got `{t}`")))
}

fn peel(call: &str) -> &str {
    let end = call.len().saturating_sub(1);
    call.get(1..end).unwrap_or("")
}

fn collection_before_find(text: &str, find_at: usize) -> Option<String> {
    let before = text.get(..find_at).unwrap_or("").trim_end();
    let before = before.strip_suffix('.').unwrap_or(before).trim_end();
    if before.is_empty() {
        return None;
    }
    if let Some(rest) = before.strip_prefix("db.") {
        let name = rest.trim();
        if !name.is_empty() && !name.contains('.') {
            return Some(name.to_string());
        }
    }
    if let Some(dot) = before.rfind('.') {
        let name = before.get(dot + 1..).unwrap_or("").trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

fn find_ident(lower: &str, name: &str) -> Option<usize> {
    let hay = lower.as_bytes();
    let needle = name.as_bytes();
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay.get(i..i + needle.len()) == Some(needle) {
            let after = i + needle.len();
            let before_ok = i == 0
                || hay
                    .get(i - 1)
                    .is_some_and(|b| !b.is_ascii_alphanumeric() && *b != b'_');
            let after_ok = hay
                .get(after)
                .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_');
            if before_ok && after_ok {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn ident_end(s: &str, start: usize) -> usize {
    let b = s.as_bytes();
    let mut i = start;
    while b
        .get(i)
        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
    {
        i += 1;
    }
    i
}

fn skip_ws(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    while b.get(i).is_some_and(|c| c.is_ascii_whitespace()) {
        i += 1;
    }
    i
}

fn split_top_level_objects(inner: &str) -> ConnectorResult<Vec<&str>> {
    let mut out = Vec::new();
    let mut i = 0;
    let n = inner.len();
    while i < n {
        i = skip_ws(inner, i);
        if i >= n {
            break;
        }
        let Some(b) = inner.as_bytes().get(i).copied() else {
            break;
        };
        if b == b',' {
            i += 1;
            continue;
        }
        if b == b'{' {
            let obj = slice_balanced(inner, i, '{', '}')?;
            i += obj.len();
            out.push(obj);
            continue;
        }
        let sample: String = inner.get(i..).unwrap_or("").chars().take(24).collect();
        return Err(ConnectorError::Query(format!(
            "find() arguments must be objects `{{ … }}` (got `{sample}`)"
        )));
    }
    Ok(out)
}

/// Inclusive slice of a balanced `open…close` starting at `start`.
pub fn slice_balanced<'a>(
    s: &'a str,
    start: usize,
    open: char,
    close: char,
) -> ConnectorResult<&'a str> {
    let rest = s
        .get(start..)
        .ok_or_else(|| ConnectorError::Query("expected object or `(`".into()))?;
    let mut chars = rest.char_indices();
    let Some((_, c0)) = chars.next() else {
        return Err(ConnectorError::Query("expected object or `(`".into()));
    };
    if c0 != open {
        return Err(ConnectorError::Query(format!(
            "expected `{open}`, got `{c0}`"
        )));
    }
    let mut depth = 1i32;
    let mut in_str: Option<char> = None;
    let mut esc = false;
    for (rel, c) in chars {
        if let Some(q) = in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == q {
                in_str = None;
            }
            continue;
        }
        if c == '"' || c == '\'' {
            in_str = Some(c);
            continue;
        }
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                let end = start + rel + c.len_utf8();
                return s.get(start..end).ok_or_else(|| {
                    ConnectorError::Query(format!("unclosed `{open}` in Mongo query"))
                });
            }
        }
    }
    Err(ConnectorError::Query(format!(
        "unclosed `{open}` in Mongo query"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_filter_projection_sort() {
        let q = r#"db.mail_notifications.find({ email: "ajaybhatt17@gmail.com" },{ title: 1, content: 1, variables: 1, is_sent: 1, createdAt: 1, sent_at: 1 }).sort({ createdAt: -1 });"#;
        let p = parse_read_query(q).expect("parse");
        assert_eq!(p.collection.as_deref(), Some("mail_notifications"));
        assert_eq!(p.filter.get_str("email").unwrap(), "ajaybhatt17@gmail.com");
        let proj = p.projection.expect("projection");
        assert_eq!(proj.get_i32("title").unwrap(), 1);
        assert_eq!(proj.get_i32("content").unwrap(), 1);
        let sort = p.sort.expect("sort");
        assert_eq!(sort.get_i32("createdAt").unwrap(), -1);
    }

    #[test]
    fn bare_filter_still_works() {
        let p = parse_read_query(r#"{ "status": "ok" }"#).unwrap();
        assert_eq!(p.filter.get_str("status").unwrap(), "ok");
        assert!(p.projection.is_none());
    }

    #[test]
    fn find_empty_and_limit() {
        let p = parse_read_query("db.users.find().limit(10)").unwrap();
        assert_eq!(p.collection.as_deref(), Some("users"));
        assert!(p.filter.is_empty());
        assert_eq!(p.limit, Some(10));
    }

    #[test]
    fn non_ascii_collection_does_not_panic() {
        let p = parse_read_query(r#"db.Zürich.find({ city: "Zürich" })"#).expect("parse");
        assert_eq!(p.collection.as_deref(), Some("Zürich"));
        assert_eq!(p.filter.get_str("city").expect("city"), "Zürich");
    }
}
