//! Compile grid filter rules into engine SQL / JSON predicates.
//! The frontend only collects rules; plugins execute this output.

mod code;
mod mongo;
mod sort;

use super::sql::{quote_ident, sql_string_literal, SqlIdentStyle};
use crate::catalog::TableFilter;
use crate::error::{ConnectorError, ConnectorResult};

pub use code::parse_code_filter_rules;
pub use mongo::mongo_filter_json;
pub use sort::{preview_sort_field, preview_sort_sql};

fn like_escape(s: &str) -> String {
    s.replace('!', "!!").replace('%', "!%").replace('_', "!_")
}

pub(super) fn looks_numeric(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    t.parse::<f64>().is_ok()
}

fn sql_eq_value(style: SqlIdentStyle, raw: &str) -> String {
    let t = raw.trim();
    if t.eq_ignore_ascii_case("null") {
        return "NULL".into();
    }
    // Always quote: `text_col = 10` errors in Postgres when any row is non-numeric.
    // `int_col = '10'` casts cleanly on PG / MySQL / SQLite.
    sql_string_literal(style, raw)
}

fn sql_cmp_value(style: SqlIdentStyle, raw: &str) -> String {
    let t = raw.trim();
    if looks_numeric(t) {
        return t.to_string();
    }
    sql_string_literal(style, raw)
}

fn sql_text_expr(style: SqlIdentStyle, ident: &str) -> String {
    match style {
        SqlIdentStyle::Mysql => format!("CAST({ident} AS CHAR)"),
        SqlIdentStyle::Postgres => format!("CAST({ident} AS TEXT)"),
    }
}

/// ` WHERE a AND b` or empty. Identifiers are quoted; strings escaped.
pub fn sql_where_clause(style: SqlIdentStyle, filters: &[TableFilter]) -> ConnectorResult<String> {
    if filters.is_empty() {
        return Ok(String::new());
    }
    let mut parts = Vec::with_capacity(filters.len());
    for f in filters {
        let col = f.column.trim();
        if col.is_empty() {
            return Err(ConnectorError::Query("filter is missing a column".into()));
        }
        if col.chars().any(|c| c == '\0' || c.is_control()) {
            return Err(ConnectorError::Query(
                "filter column is not an identifier".into(),
            ));
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
                    format!("{ident} = {}", sql_eq_value(style, &f.value))
                }
            }
            "neq" => {
                if f.value.trim().eq_ignore_ascii_case("null") {
                    format!("{ident} IS NOT NULL")
                } else {
                    format!("{ident} <> {}", sql_eq_value(style, &f.value))
                }
            }
            "gt" => format!("{ident} > {}", sql_cmp_value(style, &f.value)),
            "gte" => format!("{ident} >= {}", sql_cmp_value(style, &f.value)),
            "lt" => format!("{ident} < {}", sql_cmp_value(style, &f.value)),
            "lte" => format!("{ident} <= {}", sql_cmp_value(style, &f.value)),
            "contains" => {
                let pat = sql_string_literal(style, &format!("%{}%", like_escape(&f.value)));
                like(pat)
            }
            "starts" => {
                let pat = sql_string_literal(style, &format!("{}%", like_escape(&f.value)));
                like(pat)
            }
            "ends" => {
                let pat = sql_string_literal(style, &format!("%{}", like_escape(&f.value)));
                like(pat)
            }
            "is_null" => format!("{ident} IS NULL"),
            "not_null" => format!("{ident} IS NOT NULL"),
            "is_empty" => format!("({ident} IS NOT NULL AND {ident} = '')"),
            "not_empty" => format!("({ident} IS NOT NULL AND {ident} <> '')"),
            "json" => {
                return Err(ConnectorError::Query(
                    "JSON object filters are for MongoDB. Use column = value (e.g. email = 'a@b.com')."
                        .into(),
                ));
            }
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

/// Compile UI rules plus the raw code-filter box into ` WHERE …`.
pub fn sql_preview_where(
    style: SqlIdentStyle,
    filters: &[TableFilter],
    code_filter: Option<&str>,
) -> ConnectorResult<String> {
    let mut all = filters.to_vec();
    if let Some(code) = code_filter.map(str::trim).filter(|s| !s.is_empty()) {
        all.extend(parse_code_filter_rules(code)?);
    }
    sql_where_clause(style, &all)
}

#[cfg(test)]
mod tests;
