//! Compile grid filter rules to BSON (ObjectId + type-flexible equality).

use mongodb::bson::oid::ObjectId;
use mongodb::bson::{doc, Bson, Document, Regex};
use mystique_connector_api::{ConnectorError, ConnectorResult, TableFilter};

use crate::shell_filter::parse_filter_object;

/// UI rules plus optional raw code-filter (JSON document or `col = val`).
pub fn compile_mongo_preview_predicate(
    filters: &[TableFilter],
    code_filter: Option<&str>,
) -> ConnectorResult<Document> {
    let mut docs: Vec<Document> = Vec::new();
    if !filters.is_empty() {
        docs.push(compile_mongo_preview_filter(filters)?);
    }
    if let Some(code) = code_filter.map(str::trim).filter(|s| !s.is_empty()) {
        if code.starts_with('{') {
            docs.push(parse_filter_object(code)?);
        } else {
            let rules = mystique_connector_api::parse_code_filter_rules(code)?;
            if !rules.is_empty() {
                docs.push(compile_mongo_preview_filter(&rules)?);
            }
        }
    }
    if docs.is_empty() {
        return Ok(Document::new());
    }
    if docs.len() == 1 {
        return Ok(docs.remove(0));
    }
    Ok(doc! { "$and": docs })
}

pub fn compile_mongo_preview_filter(filters: &[TableFilter]) -> ConnectorResult<Document> {
    if filters.is_empty() {
        return Ok(Document::new());
    }
    let mut parts = Vec::with_capacity(filters.len());
    for f in filters {
        parts.push(Bson::Document(clause(f)?));
    }
    if parts.len() == 1 {
        return match parts.pop() {
            Some(Bson::Document(d)) => Ok(d),
            _ => Ok(Document::new()),
        };
    }
    Ok(doc! { "$and": parts })
}

fn clause(f: &TableFilter) -> ConnectorResult<Document> {
    let col = f.column.trim();
    let op = f.op.trim().to_ascii_lowercase();
    if op == "json" {
        return parse_filter_object(f.value.trim());
    }
    if col.is_empty() {
        return Err(ConnectorError::Query("filter is missing a column".into()));
    }
    let raw = f.value.as_str();
    match op.as_str() {
        "eq" => Ok(eq_doc(col, raw, false)),
        "neq" => Ok(eq_doc(col, raw, true)),
        "gt" => Ok(doc! { col: { "$gt": cmp_value(raw) } }),
        "gte" => Ok(doc! { col: { "$gte": cmp_value(raw) } }),
        "lt" => Ok(doc! { col: { "$lt": cmp_value(raw) } }),
        "lte" => Ok(doc! { col: { "$lte": cmp_value(raw) } }),
        "contains" => Ok(regex_doc(col, &regex_escape(raw), "")),
        "starts" => Ok(regex_doc(col, &format!("^{}", regex_escape(raw)), "")),
        "ends" => Ok(regex_doc(col, &format!("{}$", regex_escape(raw)), "")),
        "is_null" => Ok(doc! { col: Bson::Null }),
        "not_null" => Ok(doc! { col: { "$ne": Bson::Null } }),
        "is_empty" => Ok(doc! { col: "" }),
        "not_empty" => Ok(doc! { col: { "$nin": ["", Bson::Null] } }),
        other => Err(ConnectorError::Query(format!(
            "unknown table filter op `{other}`"
        ))),
    }
}

fn regex_doc(col: &str, pattern: &str, _unused: &str) -> Document {
    doc! {
        col: Bson::RegularExpression(Regex {
            pattern: pattern.to_string(),
            options: "i".into(),
        })
    }
}

fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if ".+*?^$()[]{}|\\".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn parse_oid(raw: &str) -> Option<ObjectId> {
    let t = raw.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(t) {
        if let Some(s) = v.get("$oid").and_then(|x| x.as_str()) {
            return ObjectId::parse_str(s).ok();
        }
    }
    let hex = t.trim_matches(|c| c == '"' || c == '\'');
    ObjectId::parse_str(hex).ok()
}

/// Equality must accept string / number / bool / ObjectId — the grid shows
/// ObjectIds as hex and numbers as decimal text.
fn eq_values(raw: &str) -> Vec<Bson> {
    let t = raw.trim();
    let mut vals = Vec::new();
    vals.push(Bson::String(raw.to_string()));
    if raw != t {
        vals.push(Bson::String(t.to_string()));
    }
    if let Some(oid) = parse_oid(t) {
        vals.push(Bson::ObjectId(oid));
    }
    let lower = t.to_ascii_lowercase();
    if lower == "true" {
        vals.push(Bson::Boolean(true));
    } else if lower == "false" {
        vals.push(Bson::Boolean(false));
    }
    if let Ok(n) = t.parse::<i64>() {
        if (i32::MIN as i64..=i32::MAX as i64).contains(&n) {
            vals.push(Bson::Int32(n as i32));
        }
        vals.push(Bson::Int64(n));
    } else if let Ok(n) = t.parse::<f64>() {
        vals.push(Bson::Double(n));
    }
    vals
}

fn eq_doc(col: &str, raw: &str, negate: bool) -> Document {
    if raw.trim().eq_ignore_ascii_case("null") {
        return if negate {
            doc! { col: { "$ne": Bson::Null } }
        } else {
            doc! { col: Bson::Null }
        };
    }
    let vals = eq_values(raw);
    if vals.len() == 1 {
        let v = vals.into_iter().next().unwrap();
        return if negate {
            doc! { col: { "$ne": v } }
        } else {
            doc! { col: v }
        };
    }
    if negate {
        doc! { col: { "$nin": vals } }
    } else {
        doc! { col: { "$in": vals } }
    }
}

fn cmp_value(raw: &str) -> Bson {
    let t = raw.trim();
    if let Ok(n) = t.parse::<i64>() {
        if (i32::MIN as i64..=i32::MAX as i64).contains(&n) {
            return Bson::Int32(n as i32);
        }
        return Bson::Int64(n);
    }
    if let Ok(n) = t.parse::<f64>() {
        return Bson::Double(n);
    }
    Bson::String(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::TableFilter;

    fn f(column: &str, op: &str, value: &str) -> TableFilter {
        TableFilter {
            column: column.into(),
            op: op.into(),
            value: value.into(),
        }
    }

    #[test]
    fn eq_oid_includes_object_id() {
        let hex = "64f123456789abcdef123456";
        let d = compile_mongo_preview_filter(&[f("_id", "eq", hex)]).unwrap();
        let inn = d.get("_id").unwrap().as_document().unwrap();
        let arr = inn.get("$in").unwrap().as_array().unwrap();
        assert!(arr.iter().any(|b| matches!(b, Bson::ObjectId(_))));
        assert!(arr.iter().any(|b| matches!(b, Bson::String(s) if s == hex)));
    }

    #[test]
    fn eq_number_matches_int_and_string() {
        let d = compile_mongo_preview_filter(&[f("age", "eq", "10")]).unwrap();
        let inn = d.get("age").unwrap().as_document().unwrap();
        let arr = inn.get("$in").unwrap().as_array().unwrap();
        assert!(arr.iter().any(|b| matches!(b, Bson::Int32(10))));
        assert!(arr
            .iter()
            .any(|b| matches!(b, Bson::String(s) if s == "10")));
    }

    #[test]
    fn json_object_filter() {
        let d = compile_mongo_preview_filter(&[f(
            "*",
            "json",
            r#"{ "email": "ajaybhatt17@gmail.com" }"#,
        )])
        .unwrap();
        assert_eq!(d.get_str("email").unwrap(), "ajaybhatt17@gmail.com");
    }

    #[test]
    fn json_shell_object_filter() {
        let d =
            compile_mongo_preview_filter(&[f("*", "json", r#"{ email: "ada@x.com" }"#)]).unwrap();
        assert_eq!(d.get_str("email").unwrap(), "ada@x.com");
    }

    #[test]
    fn code_filter_json_document() {
        let d =
            compile_mongo_preview_predicate(&[], Some(r#"{ "email": "ajaybhatt17@gmail.com" }"#))
                .unwrap();
        assert_eq!(d.get_str("email").unwrap(), "ajaybhatt17@gmail.com");
    }
}
