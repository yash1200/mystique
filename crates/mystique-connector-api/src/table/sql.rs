//! Relational compile helpers for [`apply_table_edits`].
//! Plugins still execute the SQL; this only builds dialect-aware statements.

use serde_json::Value as JsonValue;

mod dml;
mod plan;

use super::edits::TableEditColumn;

pub use dml::{compile_delete, compile_insert, compile_update, compile_upsert};
pub use plan::{
    affected_mismatch, keyless_ambiguous, overlay_catalog_pk, plan_table_edits, EditPlan,
    PlannedCount, PlannedEdit, AFFECTED_MISMATCH, KEYLESS_AMBIGUOUS,
};

#[derive(Debug, Clone, Copy)]
pub enum SqlIdentStyle {
    Postgres,
    Mysql,
}

pub fn quote_ident(style: SqlIdentStyle, id: &str) -> String {
    match style {
        SqlIdentStyle::Postgres => format!("\"{}\"", id.replace('"', "\"\"")),
        SqlIdentStyle::Mysql => format!("`{}`", id.replace('`', "``")),
    }
}

pub fn qualify_table(
    style: SqlIdentStyle,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) -> String {
    let t = quote_ident(style, table);
    match style {
        SqlIdentStyle::Mysql => {
            let db = schema
                .filter(|s| !s.is_empty())
                .or_else(|| database.filter(|s| !s.is_empty()));
            match db {
                Some(d) => format!("{}.{}", quote_ident(style, d), t),
                None => t,
            }
        }
        SqlIdentStyle::Postgres => match schema.filter(|s| !s.is_empty()) {
            Some(s) => format!("{}.{}", quote_ident(style, s), t),
            None => t,
        },
    }
}

fn is_numeric_type(t: &str) -> bool {
    let t = t.to_ascii_lowercase();
    t.contains("int")
        || t.contains("serial")
        || t.contains("numeric")
        || t.contains("decimal")
        || t.contains("float")
        || t.contains("double")
        || t.contains("real")
        || t.contains("money")
        || t.contains("number")
        || t.contains("oid")
}

fn is_bool_type(t: &str) -> bool {
    let t = t.to_ascii_lowercase();
    t == "bool" || t == "boolean" || t == "bit" || t == "bit(1)" || t.contains("tinyint(1)")
}

fn bool_sql(style: SqlIdentStyle, v: bool) -> &'static str {
    match style {
        SqlIdentStyle::Mysql => {
            if v {
                "1"
            } else {
                "0"
            }
        }
        SqlIdentStyle::Postgres => {
            if v {
                "TRUE"
            } else {
                "FALSE"
            }
        }
    }
}

/// One SQL string literal. MySQL also escapes `\` because `\'` is a quote there.
pub fn sql_string_literal(style: SqlIdentStyle, s: &str) -> String {
    match style {
        SqlIdentStyle::Mysql => {
            let mut out = String::with_capacity(s.len() + 2);
            out.push('\'');
            for c in s.chars() {
                match c {
                    '\\' => out.push_str("\\\\"),
                    '\'' => out.push_str("''"),
                    '\0' => out.push_str("\\0"),
                    _ => out.push(c),
                }
            }
            out.push('\'');
            out
        }
        SqlIdentStyle::Postgres => format!("'{}'", s.replace('\'', "''")),
    }
}

pub fn sql_literal(style: SqlIdentStyle, value: &JsonValue, data_type: &str) -> String {
    match value {
        JsonValue::Null => "NULL".into(),
        JsonValue::Bool(b) => bool_sql(style, *b).into(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::String(s) => {
            if is_bool_type(data_type) {
                let b = matches!(s.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "t");
                return bool_sql(style, b).into();
            }
            let t = s.trim();
            if is_numeric_type(data_type) && !t.is_empty() && is_plain_number(t) {
                return t.to_string();
            }
            sql_string_literal(style, s)
        }
        other => sql_string_literal(style, &other.to_string()),
    }
}

fn is_plain_number(s: &str) -> bool {
    let mut chars = s.chars().peekable();
    if matches!(chars.peek(), Some('+') | Some('-')) {
        chars.next();
    }
    let rest: String = chars.collect();
    if rest.is_empty() {
        return false;
    }
    let mut seen_dot = false;
    rest.chars().all(|c| {
        if c == '.' {
            if seen_dot {
                return false;
            }
            seen_dot = true;
            true
        } else {
            c.is_ascii_digit()
        }
    })
}

pub(super) fn col_type<'a>(columns: &'a [TableEditColumn], name: &str) -> &'a str {
    columns
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.data_type.as_str())
        .unwrap_or("")
}

pub(super) fn pk_columns(columns: &[TableEditColumn]) -> Vec<&TableEditColumn> {
    let pks: Vec<_> = columns.iter().filter(|c| c.pk).collect();
    if !pks.is_empty() {
        return pks;
    }
    columns
        .iter()
        .filter(|c| c.name == "id" || c.name == "_id" || c.name == "rowid")
        .collect()
}

/// `SELECT DISTINCT` for the grid filter. The column must be one the catalog returned.
pub fn distinct_values_sql(
    style: SqlIdentStyle,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    column: &str,
    known: &[crate::ColumnInfo],
    limit: u32,
) -> crate::ConnectorResult<String> {
    if !known.iter().any(|info| info.name == column) {
        return Err(crate::ConnectorError::Query(format!(
            "Unknown column “{column}”."
        )));
    }
    let limit = limit.clamp(1, 1_000);
    let ident = quote_ident(style, column);
    let target = qualify_table(style, database, schema, table);
    Ok(format!(
        "SELECT DISTINCT {ident} FROM {target} WHERE {ident} IS NOT NULL LIMIT {limit}"
    ))
}

#[cfg(test)]
mod distinct_tests {
    use super::distinct_values_sql;
    use super::SqlIdentStyle;
    use crate::ColumnInfo;

    fn column(name: &str) -> ColumnInfo {
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
    fn distinct_quotes_a_known_column_and_caps_the_limit() {
        let sql = distinct_values_sql(
            SqlIdentStyle::Postgres,
            None,
            Some("public"),
            "docs",
            "body",
            &[column("id"), column("body")],
            5_000,
        )
        .expect("sql");
        assert_eq!(
            sql,
            "SELECT DISTINCT \"body\" FROM \"public\".\"docs\" WHERE \"body\" IS NOT NULL LIMIT 1000"
        );
        assert!(distinct_values_sql(
            SqlIdentStyle::Mysql,
            Some("app"),
            None,
            "docs",
            "nope",
            &[column("id")],
            10,
        )
        .is_err());
    }
}

pub(super) fn is_empty_value(v: &JsonValue) -> bool {
    match v {
        JsonValue::Null => true,
        JsonValue::String(s) => s.is_empty(),
        _ => false,
    }
}
