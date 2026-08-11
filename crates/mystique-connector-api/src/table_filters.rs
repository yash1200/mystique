//! Compile grid filter rules into engine SQL / JSON predicates.
//! The frontend only collects rules; plugins execute this output.

use crate::catalog::TableFilter;
use crate::error::{ConnectorError, ConnectorResult};
use crate::sql_table_edits::{quote_ident, SqlIdentStyle};

fn sql_string_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn like_escape(s: &str) -> String {
    s.replace('!', "!!").replace('%', "!%").replace('_', "!_")
}

fn looks_numeric(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    t.parse::<f64>().is_ok()
}

fn sql_eq_value(raw: &str) -> String {
    let t = raw.trim();
    if t.eq_ignore_ascii_case("null") {
        return "NULL".into();
    }
    // Always quote: `text_col = 10` errors in Postgres when any row is non-numeric.
    // `int_col = '10'` casts cleanly on PG / MySQL / SQLite.
    sql_string_literal(raw)
}

fn sql_cmp_value(raw: &str) -> String {
    let t = raw.trim();
    if looks_numeric(t) {
        return t.to_string();
    }
    sql_string_literal(raw)
}

fn sql_text_expr(style: SqlIdentStyle, ident: &str) -> String {
    match style {
        SqlIdentStyle::Mysql => format!("CAST({ident} AS CHAR)"),
        SqlIdentStyle::Postgres => format!("CAST({ident} AS TEXT)"),
    }
}

/// ` WHERE a AND b` or empty. Identifiers are quoted; strings escaped.
pub fn sql_where_clause(
    style: SqlIdentStyle,
    filters: &[TableFilter],
) -> ConnectorResult<String> {
    if filters.is_empty() {
        return Ok(String::new());
    }
    let mut parts = Vec::with_capacity(filters.len());
    for f in filters {
        let col = f.column.trim();
        if col.is_empty() {
            return Err(ConnectorError::Query("filter is missing a column".into()));
        }
        let ident = quote_ident(style, col);
        let op = f.op.trim().to_ascii_lowercase();
        let text_expr = sql_text_expr(style, &ident);
        let like = |pat: String| -> String {
            // CAST to text so LIKE works on ints / uuid / jsonb. LOWER is ASCII-CI.
            format!("LOWER({text_expr}) LIKE LOWER({pat}) ESCAPE '!'")
        };
        let piece = match op.as_str() {
            "eq" => {
                if f.value.trim().eq_ignore_ascii_case("null") {
                    format!("{ident} IS NULL")
                } else {
                    format!("{ident} = {}", sql_eq_value(&f.value))
                }
            }
            "neq" => {
                if f.value.trim().eq_ignore_ascii_case("null") {
                    format!("{ident} IS NOT NULL")
                } else {
                    format!("{ident} <> {}", sql_eq_value(&f.value))
                }
            }
            "gt" => format!("{ident} > {}", sql_cmp_value(&f.value)),
            "gte" => format!("{ident} >= {}", sql_cmp_value(&f.value)),
            "lt" => format!("{ident} < {}", sql_cmp_value(&f.value)),
            "lte" => format!("{ident} <= {}", sql_cmp_value(&f.value)),
            "contains" => {
                let pat = sql_string_literal(&format!("%{}%", like_escape(&f.value)));
                like(pat)
            }
            "starts" => {
                let pat = sql_string_literal(&format!("{}%", like_escape(&f.value)));
                like(pat)
            }
            "ends" => {
                let pat = sql_string_literal(&format!("%{}", like_escape(&f.value)));
                like(pat)
            }
            "is_null" => format!("{ident} IS NULL"),
            "not_null" => format!("{ident} IS NOT NULL"),
            "is_empty" => format!("({ident} IS NOT NULL AND {ident} = '')"),
            "not_empty" => format!("({ident} IS NOT NULL AND {ident} <> '')"),
            other => {
                return Err(ConnectorError::Query(format!(
                    "unknown table filter op `{other}`"
                )));
            }
        };
        parts.push(piece);
    }
    Ok(format!(" WHERE {}", parts.join(" AND ")))
}

/// JSON object the Mongo plugin turns into a BSON filter (`$and` when needed).
pub fn mongo_filter_json(filters: &[TableFilter]) -> ConnectorResult<serde_json::Value> {
    use serde_json::{json, Value};
    if filters.is_empty() {
        return Ok(json!({}));
    }
    let mut clauses = Vec::with_capacity(filters.len());
    for f in filters {
        let col = f.column.trim();
        if col.is_empty() {
            return Err(ConnectorError::Query("filter is missing a column".into()));
        }
        let op = f.op.trim().to_ascii_lowercase();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn f(column: &str, op: &str, value: &str) -> TableFilter {
        TableFilter {
            column: column.into(),
            op: op.into(),
            value: value.into(),
        }
    }

    #[test]
    fn sql_eq_and_like() {
        let where_sql = sql_where_clause(
            SqlIdentStyle::Postgres,
            &[f("name", "contains", "a_b"), f("age", "gt", "10")],
        )
        .unwrap();
        assert!(where_sql.contains("LOWER(CAST(\"name\" AS TEXT)) LIKE LOWER('%a!_b%') ESCAPE '!'"));
        assert!(where_sql.contains("\"age\" > 10"));
    }

    #[test]
    fn sql_eq_quotes_numeric_looking_text() {
        let where_sql =
            sql_where_clause(SqlIdentStyle::Postgres, &[f("status", "eq", "10")]).unwrap();
        assert_eq!(where_sql, " WHERE \"status\" = '10'");
    }

    #[test]
    fn sql_quotes_ident_and_string() {
        let where_sql =
            sql_where_clause(SqlIdentStyle::Mysql, &[f("na`me", "eq", "O'Brien")]).unwrap();
        assert_eq!(where_sql, " WHERE `na``me` = 'O''Brien'");
    }
}
