//! Compile grid filter rules into engine SQL / JSON predicates.
//! The frontend only collects rules; plugins execute this output.

use crate::catalog::{ColumnInfo, TableFilter};
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

/// Parse the code-filter box: JSON `{ "col": "val" }` or `col = val AND …`.
pub fn parse_code_filter_rules(code: &str) -> ConnectorResult<Vec<TableFilter>> {
    let t = code.trim();
    if t.is_empty() {
        return Ok(Vec::new());
    }
    if t.starts_with('{') {
        return object_to_eq_filters(t);
    }
    parse_predicate_lines(t)
}

fn object_to_eq_filters(raw: &str) -> ConnectorResult<Vec<TableFilter>> {
    let v = parse_loose_json_object(raw)?;
    let obj = v.as_object().ok_or_else(|| {
        ConnectorError::Query("code filter JSON must be an object `{ column: value }`".into())
    })?;
    let mut out = Vec::with_capacity(obj.len());
    for (k, val) in obj {
        let value = match val {
            serde_json::Value::Null => "null".into(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            _ => {
                return Err(ConnectorError::Query(format!(
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

fn parse_loose_json_object(raw: &str) -> ConnectorResult<serde_json::Value> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        return Ok(v);
    }
    let quoted = quote_unquoted_json_keys(raw);
    serde_json::from_str(&quoted)
        .map_err(|e| ConnectorError::Query(format!("invalid code filter JSON: {e}")))
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

fn parse_predicate_lines(code: &str) -> ConnectorResult<Vec<TableFilter>> {
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

fn parse_one_predicate(line: &str) -> ConnectorResult<(String, String, String)> {
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
    Err(ConnectorError::Query(format!(
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

/// Resolve a requested sort column against described columns (exact name).
pub fn resolve_preview_sort_column<'a>(
    columns: &'a [ColumnInfo],
    sort_column: Option<&str>,
) -> Option<&'a str> {
    let name = sort_column.map(str::trim).filter(|s| !s.is_empty())?;
    columns
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.name.as_str())
}

fn preview_sort_dir_desc(sort_dir: Option<&str>) -> bool {
    matches!(
        sort_dir
            .unwrap_or("asc")
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "desc" | "descending" | "-1"
    )
}

/// ` ORDER BY "col" ASC` when the column is known; `None` = plugin default order.
pub fn preview_sort_sql(
    style: SqlIdentStyle,
    columns: &[ColumnInfo],
    sort_column: Option<&str>,
    sort_dir: Option<&str>,
) -> Option<String> {
    let col = resolve_preview_sort_column(columns, sort_column)?;
    let dir = if preview_sort_dir_desc(sort_dir) {
        "DESC"
    } else {
        "ASC"
    };
    Some(format!(" ORDER BY {} {}", quote_ident(style, col), dir))
}

/// Mongo `sort` field + direction (`1` / `-1`) when the column is known.
pub fn preview_sort_field<'a>(
    columns: &'a [ColumnInfo],
    sort_column: Option<&str>,
    sort_dir: Option<&str>,
) -> Option<(&'a str, i32)> {
    let col = resolve_preview_sort_column(columns, sort_column)?;
    let dir = if preview_sort_dir_desc(sort_dir) {
        -1
    } else {
        1
    };
    Some((col, dir))
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

    #[test]
    fn code_filter_json_equals() {
        let where_sql = sql_preview_where(
            SqlIdentStyle::Postgres,
            &[],
            Some(r#"{ "email": "ajay@x.com" }"#),
        )
        .unwrap();
        assert_eq!(where_sql, " WHERE \"email\" = 'ajay@x.com'");
    }

    #[test]
    fn code_filter_expression() {
        let rules = parse_code_filter_rules("status = 'active' AND age > 10").unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].column, "status");
        assert_eq!(rules[0].op, "eq");
        assert_eq!(rules[1].op, "gt");
    }

    fn col(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
            pk: false,
            enum_values: None,
            default_value: None,
            references: None,
        }
    }

    #[test]
    fn preview_sort_sql_quotes_and_dir() {
        let cols = [col("na\"me")];
        let sql =
            preview_sort_sql(SqlIdentStyle::Postgres, &cols, Some("na\"me"), Some("desc")).unwrap();
        assert_eq!(sql, " ORDER BY \"na\"\"me\" DESC");
    }

    #[test]
    fn preview_sort_sql_unknown_column_is_none() {
        let cols = [col("id")];
        assert!(
            preview_sort_sql(SqlIdentStyle::Postgres, &cols, Some("nope"), Some("asc")).is_none()
        );
        assert!(preview_sort_sql(SqlIdentStyle::Postgres, &cols, None, Some("desc")).is_none());
    }

    #[test]
    fn preview_sort_field_defaults_asc() {
        let cols = [col("age")];
        assert_eq!(
            preview_sort_field(&cols, Some("age"), None),
            Some(("age", 1))
        );
        assert_eq!(
            preview_sort_field(&cols, Some("age"), Some("DESC")),
            Some(("age", -1))
        );
    }
}
