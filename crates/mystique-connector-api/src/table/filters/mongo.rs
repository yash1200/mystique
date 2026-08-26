//! Compile UI filter rules to a Mongo JSON predicate.

use crate::catalog::TableFilter;
use crate::error::{ConnectorError, ConnectorResult};

use super::looks_numeric;

/// JSON object the Mongo plugin turns into a BSON filter (`$and` when needed).
pub fn mongo_filter_json(filters: &[TableFilter]) -> ConnectorResult<serde_json::Value> {
    use serde_json::{json, Value};
    if filters.is_empty() {
        return Ok(json!({}));
    }
    let mut clauses = Vec::with_capacity(filters.len());
    for f in filters {
        let col = f.column.trim();
        let op = f.op.trim().to_ascii_lowercase();
        if op == "json" {
            let v: Value = serde_json::from_str(f.value.trim())
                .map_err(|e| ConnectorError::Query(format!("invalid JSON filter: {e}")))?;
            clauses.push(v);
            continue;
        }
        if col.is_empty() {
            return Err(ConnectorError::Query("filter is missing a column".into()));
        }
        let as_json = || -> Value {
            let t = f.value.trim();
            if t.eq_ignore_ascii_case("null") {
                return Value::Null;
            }
            if looks_numeric(t) {
                if let Ok(n) = t.parse::<i64>() {
                    return json!(n);
                }
                if let Ok(n) = t.parse::<f64>() {
                    return json!(n);
                }
            }
            let lower = t.to_ascii_lowercase();
            if lower == "true" {
                return json!(true);
            }
            if lower == "false" {
                return json!(false);
            }
            json!(f.value)
        };
        let regex_escape = |s: &str| {
            let mut out = String::new();
            for c in s.chars() {
                if ".+*?^$()[]{}|\\".contains(c) {
                    out.push('\\');
                }
                out.push(c);
            }
            out
        };
        let clause = match op.as_str() {
            "eq" => {
                if f.value.trim().eq_ignore_ascii_case("null") || f.value.is_empty() {
                    json!({ col: null })
                } else {
                    json!({ col: as_json() })
                }
            }
            "neq" => json!({ col: { "$ne": as_json() } }),
            "gt" => json!({ col: { "$gt": as_json() } }),
            "gte" => json!({ col: { "$gte": as_json() } }),
            "lt" => json!({ col: { "$lt": as_json() } }),
            "lte" => json!({ col: { "$lte": as_json() } }),
            "contains" => {
                json!({ col: { "$regex": regex_escape(&f.value), "$options": "i" } })
            }
            "starts" => {
                json!({ col: { "$regex": format!("^{}", regex_escape(&f.value)), "$options": "i" } })
            }
            "ends" => {
                json!({ col: { "$regex": format!("{}$", regex_escape(&f.value)), "$options": "i" } })
            }
            "is_null" => json!({ col: null }),
            "not_null" => json!({ col: { "$ne": null } }),
            "is_empty" => json!({ col: "" }),
            "not_empty" => json!({ col: { "$nin": ["", null] } }),
            other => {
                return Err(ConnectorError::Query(format!(
                    "unknown table filter op `{other}`"
                )));
            }
        };
        clauses.push(clause);
    }
    if clauses.len() == 1 {
        return Ok(clauses.remove(0));
    }
    Ok(json!({ "$and": clauses }))
}
