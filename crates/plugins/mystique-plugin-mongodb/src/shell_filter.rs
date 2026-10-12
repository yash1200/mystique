//! Parse Mongo shell-style filter objects into BSON documents.
//! Supports unquoted keys, ObjectId("…"), ISODate("…"), NumberLong, trailing commas.

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Bson, Document};
use mystique_connector_api::{ConnectorError, ConnectorResult};
use serde_json::Value as JsonValue;

pub fn parse_filter_object(slice: &str) -> ConnectorResult<Document> {
    let slice = slice.trim();
    // Strict JSON first
    if let Ok(v) = serde_json::from_str::<JsonValue>(slice) {
        return json_value_to_document(&v);
    }
    // Shell → JSON → BSON
    let normalized = normalize_mongo_shell_object(slice)?;
    let v: JsonValue = serde_json::from_str(&normalized).map_err(|e| {
        ConnectorError::Query(format!(
            "invalid find filter (shell syntax). After normalize: {normalized} — error: {e}"
        ))
    })?;
    json_value_to_document(&v)
}

fn normalize_mongo_shell_object(input: &str) -> ConnectorResult<String> {
    let mut s = input.trim().to_string();
    s = replace_ctor(&s, "ObjectId", |arg| {
        let hex = trim_quotes(arg.trim());
        if hex.len() != 24 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ConnectorError::Query(format!(
                "invalid ObjectId (need 24 hex chars): {arg}"
            )));
        }
        Ok(format!(r#"{{"$oid":"{hex}"}}"#))
    })?;
    s = replace_ctor(&s, "ISODate", |arg| {
        let d = trim_quotes(arg.trim());
        Ok(format!(r#"{{"$date":"{d}"}}"#))
    })?;
    // `new Date("...")` — treat Date( the same
    s = replace_ctor(&s, "Date", |arg| {
        let d = trim_quotes(arg.trim());
        Ok(format!(r#"{{"$date":"{d}"}}"#))
    })?;
    s = replace_ctor(&s, "NumberLong", |arg| {
        let n = trim_quotes(arg.trim());
        Ok(format!(r#"{{"$numberLong":"{n}"}}"#))
    })?;
    s = replace_ctor(&s, "NumberInt", |arg| {
        Ok(trim_quotes(arg.trim()).to_string())
    })?;
    s = quote_unquoted_keys(&s);
    s = strip_trailing_commas(&s);
    Ok(s)
}

fn trim_quotes(s: &str) -> &str {
    s.trim_matches(|c| c == '"' || c == '\'')
}

fn replace_ctor(
    input: &str,
    name: &str,
    mut f: impl FnMut(&str) -> ConnectorResult<String>,
) -> ConnectorResult<String> {
    let input_lower = input.to_ascii_lowercase();
    let name_lower = name.to_ascii_lowercase();
    let mut out = String::with_capacity(input.len() + 32);
    let mut i = 0;
    let b = input.as_bytes();

    while i < b.len() {
        let Some(tail) = input_lower.get(i..) else {
            return Err(ConnectorError::Query(
                "filter slice was not on a character boundary".into(),
            ));
        };
        if let Some(rel) = tail.find(&name_lower) {
            let abs = i + rel;
            let boundary_ok = abs == 0
                || b.get(abs - 1)
                    .is_none_or(|c| !c.is_ascii_alphanumeric() && *c != b'_' && *c != b'.');
            // skip `new ` prefix handling: "new Date" — name Date matches after new
            if boundary_ok {
                let mut j = abs + name.len();
                while b.get(j).is_some_and(|c| c.is_ascii_whitespace()) {
                    j += 1;
                }
                if b.get(j) == Some(&b'(') {
                    let Some(prefix) = input.get(i..abs) else {
                        return Err(ConnectorError::Query(
                            "filter slice was not on a character boundary".into(),
                        ));
                    };
                    out.push_str(prefix);
                    j += 1;
                    let arg_start = j;
                    let mut depth = 1usize;
                    let mut in_str: Option<u8> = None;
                    let mut esc = false;
                    while j < b.len() && depth > 0 {
                        let Some(c) = b.get(j).copied() else {
                            break;
                        };
                        if let Some(q) = in_str {
                            if esc {
                                esc = false;
                            } else if c == b'\\' {
                                esc = true;
                            } else if c == q {
                                in_str = None;
                            }
                        } else if c == b'"' || c == b'\'' {
                            in_str = Some(c);
                        } else if c == b'(' {
                            depth += 1;
                        } else if c == b')' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        j += 1;
                    }
                    if depth != 0 {
                        return Err(ConnectorError::Query(format!(
                            "unclosed {name}(…) in filter"
                        )));
                    }
                    let Some(arg) = input.get(arg_start..j) else {
                        return Err(ConnectorError::Query(
                            "filter slice was not on a character boundary".into(),
                        ));
                    };
                    out.push_str(&f(arg)?);
                    i = j + 1;
                    continue;
                }
            }
            // not a call — copy one char past i and continue search later
            let Some(ch) = input.get(i..).and_then(|rest| rest.chars().next()) else {
                break;
            };
            out.push(ch);
            i += ch.len_utf8();
        } else {
            let Some(rest) = input.get(i..) else {
                break;
            };
            out.push_str(rest);
            break;
        }
    }
    Ok(out)
}

fn quote_unquoted_keys(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len() + 16);
    let mut i = 0;
    let mut in_str: Option<char> = None;
    let mut esc = false;

    while let Some(c) = chars.get(i).copied() {
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
            out.push(c);
            i += 1;
            continue;
        }
        if c == '_' || c == '$' || c.is_ascii_alphabetic() {
            let start = i;
            i += 1;
            while chars
                .get(i)
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
            {
                i += 1;
            }
            let ident: String = chars.get(start..i).unwrap_or(&[]).iter().collect();
            let mut j = i;
            while chars.get(j).is_some_and(|ch| ch.is_whitespace()) {
                j += 1;
            }
            if chars.get(j).copied() == Some(':') {
                // bare key → "key"
                out.push('"');
                out.push_str(&ident);
                out.push('"');
                for ch in chars.get(i..j).unwrap_or(&[]) {
                    out.push(*ch);
                }
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

fn strip_trailing_commas(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let mut in_str: Option<char> = None;
    let mut esc = false;
    while let Some(c) = chars.get(i).copied() {
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
            out.push(c);
            i += 1;
            continue;
        }
        if c == ',' {
            let mut j = i + 1;
            while chars.get(j).is_some_and(|ch| ch.is_whitespace()) {
                j += 1;
            }
            if matches!(chars.get(j).copied(), Some('}' | ']')) {
                i += 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn json_value_to_document(v: &JsonValue) -> ConnectorResult<Document> {
    match json_value_to_bson(v)? {
        Bson::Document(d) => Ok(d),
        _ => Err(ConnectorError::Query("filter must be an object".into())),
    }
}

/// Convert a JSON object (including `$oid` / `$date`) into a BSON document.
pub fn json_to_document(v: &JsonValue) -> ConnectorResult<Document> {
    json_value_to_document(v)
}

fn json_value_to_bson(v: &JsonValue) -> ConnectorResult<Bson> {
    match v {
        JsonValue::Null => Ok(Bson::Null),
        JsonValue::Bool(b) => Ok(Bson::Boolean(*b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                if (i32::MIN as i64..=i32::MAX as i64).contains(&i) {
                    Ok(Bson::Int32(i as i32))
                } else {
                    Ok(Bson::Int64(i))
                }
            } else if let Some(f) = n.as_f64() {
                Ok(Bson::Double(f))
            } else {
                Ok(Bson::String(n.to_string()))
            }
        }
        JsonValue::String(s) => Ok(Bson::String(s.clone())),
        JsonValue::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(json_value_to_bson(item)?);
            }
            Ok(Bson::Array(out))
        }
        JsonValue::Object(map) => {
            if map.len() == 1 {
                if let Some(oid) = map.get("$oid").and_then(|x| x.as_str()) {
                    let oid = ObjectId::parse_str(oid)
                        .map_err(|e| ConnectorError::Query(format!("invalid ObjectId: {e}")))?;
                    return Ok(Bson::ObjectId(oid));
                }
                if let Some(d) = map.get("$date") {
                    if let Some(s) = d.as_str() {
                        if let Ok(dt) = mongodb::bson::DateTime::parse_rfc3339_str(s) {
                            return Ok(Bson::DateTime(dt));
                        }
                        // allow non-RFC strings as plain string fallback
                        return Ok(Bson::String(s.to_string()));
                    }
                    if let Some(ms) = d.as_i64() {
                        return Ok(Bson::DateTime(mongodb::bson::DateTime::from_millis(ms)));
                    }
                }
                if let Some(n) = map.get("$numberLong").and_then(|x| x.as_str()) {
                    let i: i64 = n
                        .parse()
                        .map_err(|e| ConnectorError::Query(format!("invalid NumberLong: {e}")))?;
                    return Ok(Bson::Int64(i));
                }
            }
            let mut doc = Document::new();
            for (k, val) in map {
                doc.insert(k.clone(), json_value_to_bson(val)?);
            }
            Ok(Bson::Document(doc))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_object_id_filter() {
        let d = parse_filter_object(
            r#"{
  _id: ObjectId("64f123456789abcdef123456")
}"#,
        )
        .expect("parse");
        let oid = d.get_object_id("_id").expect("oid field");
        assert_eq!(oid.to_hex(), "64f123456789abcdef123456");
    }

    #[test]
    fn empty_and_json() {
        assert!(parse_filter_object("{}").unwrap().is_empty());
        let d = parse_filter_object(r#"{"status":"active"}"#).unwrap();
        assert_eq!(d.get_str("status").unwrap(), "active");
    }

    #[test]
    fn unquoted_string_value() {
        // values still need quotes in shell for strings — only keys unquoted
        let d = parse_filter_object(r#"{ name: "ada" }"#).unwrap();
        assert_eq!(d.get_str("name").unwrap(), "ada");
    }
}
