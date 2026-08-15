//! Relational compile helpers for [`apply_table_edits`].
//! Plugins still execute the SQL; this only builds dialect-aware statements.

use serde_json::Value as JsonValue;

use crate::edits::{ApplyTableEditsInput, TableEditColumn};
use crate::error::{ConnectorError, ConnectorResult};

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
            match style {
                SqlIdentStyle::Mysql => {
                    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "''"))
                }
                SqlIdentStyle::Postgres => format!("'{}'", s.replace('\'', "''")),
            }
        }
        other => {
            let s = other.to_string();
            match style {
                SqlIdentStyle::Mysql => {
                    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "''"))
                }
                SqlIdentStyle::Postgres => format!("'{}'", s.replace('\'', "''")),
            }
        }
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

fn col_type<'a>(columns: &'a [TableEditColumn], name: &str) -> &'a str {
    columns
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.data_type.as_str())
        .unwrap_or("")
}

fn pk_columns(columns: &[TableEditColumn]) -> Vec<&TableEditColumn> {
    let pks: Vec<_> = columns.iter().filter(|c| c.pk).collect();
    if !pks.is_empty() {
        return pks;
    }
    columns
        .iter()
        .filter(|c| c.name == "id" || c.name == "_id" || c.name == "rowid")
        .collect()
}

fn is_empty_value(v: &JsonValue) -> bool {
    match v {
        JsonValue::Null => true,
        JsonValue::String(s) => s.is_empty(),
        _ => false,
    }
}

pub fn compile_update(
    style: SqlIdentStyle,
    input: &ApplyTableEditsInput,
    update: &crate::edits::TableRowUpdate,
) -> ConnectorResult<String> {
    let pks = pk_columns(&input.columns);
    if pks.is_empty() {
        return Err(ConnectorError::Query(format!(
            "Cannot save “{}”: no primary key. Add a PK to update rows safely.",
            input.table
        )));
    }
    if update.values.is_empty() {
        return Err(ConnectorError::Query(
            "Update has no changed columns.".into(),
        ));
    }
    let target = qualify_table(
        style,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    );
    let set_sql: Vec<String> = update
        .values
        .iter()
        .map(|(name, val)| {
            format!(
                "{} = {}",
                quote_ident(style, name),
                sql_literal(style, val, col_type(&input.columns, name))
            )
        })
        .collect();
    let mut where_sql = Vec::new();
    for pk in pks {
        let v = update.original.get(&pk.name).ok_or_else(|| {
            ConnectorError::Query(format!(
                "Cannot save row: primary key “{}” missing from original row.",
                pk.name
            ))
        })?;
        if is_empty_value(v) {
            return Err(ConnectorError::Query(format!(
                "Cannot save row: primary key “{}” is empty on the original row.",
                pk.name
            )));
        }
        where_sql.push(format!(
            "{} = {}",
            quote_ident(style, &pk.name),
            sql_literal(style, v, &pk.data_type)
        ));
    }
    Ok(format!(
        "UPDATE {target} SET {} WHERE {}",
        set_sql.join(", "),
        where_sql.join(" AND ")
    ))
}

pub fn compile_insert(
    style: SqlIdentStyle,
    input: &ApplyTableEditsInput,
    insert: &crate::edits::TableRowInsert,
) -> ConnectorResult<String> {
    let cols: Vec<(&String, &JsonValue)> = insert
        .values
        .iter()
        .filter(|(_, v)| !is_empty_value(v))
        .collect();
    if cols.is_empty() {
        return Err(ConnectorError::Query(
            "New row is empty — fill at least one column before saving.".into(),
        ));
    }
    let target = qualify_table(
        style,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    );
    let names: Vec<String> = cols.iter().map(|(n, _)| quote_ident(style, n)).collect();
    let values: Vec<String> = cols
        .iter()
        .map(|(n, v)| sql_literal(style, v, col_type(&input.columns, n)))
        .collect();
    Ok(format!(
        "INSERT INTO {target} ({}) VALUES ({})",
        names.join(", "),
        values.join(", ")
    ))
}

/// Insert, or replace existing PK / unique-key rows.
pub fn compile_upsert(
    style: SqlIdentStyle,
    input: &ApplyTableEditsInput,
    insert: &crate::edits::TableRowInsert,
) -> ConnectorResult<String> {
    let insert_sql = compile_insert(style, input, insert)?;
    let pks = pk_columns(&input.columns);
    if pks.is_empty() {
        return Err(ConnectorError::Query(format!(
            "Update on duplicate needs a primary key on “{}”.",
            input.table
        )));
    }
    let pk_names: Vec<&str> = pks.iter().map(|c| c.name.as_str()).collect();
    let set_cols: Vec<&String> = insert
        .values
        .iter()
        .filter(|(n, v)| !pk_names.contains(&n.as_str()) && !is_empty_value(v))
        .map(|(n, _)| n)
        .collect();
    match style {
        SqlIdentStyle::Mysql => {
            if set_cols.is_empty() {
                return Ok(insert_sql);
            }
            let sets: Vec<String> = set_cols
                .iter()
                .map(|c| {
                    let q = quote_ident(style, c);
                    format!("{q} = VALUES({q})")
                })
                .collect();
            Ok(format!("{insert_sql} ON DUPLICATE KEY UPDATE {}", sets.join(", ")))
        }
        SqlIdentStyle::Postgres => {
            let conflict = pks
                .iter()
                .map(|c| quote_ident(style, &c.name))
                .collect::<Vec<_>>()
                .join(", ");
            if set_cols.is_empty() {
                return Ok(format!("{insert_sql} ON CONFLICT ({conflict}) DO NOTHING"));
            }
            let sets: Vec<String> = set_cols
                .iter()
                .map(|c| {
                    let q = quote_ident(style, c);
                    format!("{q} = EXCLUDED.{q}")
                })
                .collect();
            Ok(format!(
                "{insert_sql} ON CONFLICT ({conflict}) DO UPDATE SET {}",
                sets.join(", ")
            ))
        }
    }
}

pub fn compile_delete(
    style: SqlIdentStyle,
    input: &ApplyTableEditsInput,
    delete: &crate::edits::TableRowDelete,
) -> ConnectorResult<String> {
    let pks = pk_columns(&input.columns);
    if pks.is_empty() {
        return Err(ConnectorError::Query(format!(
            "Cannot delete from “{}”: no primary key. Add a PK to delete rows safely.",
            input.table
        )));
    }
    let target = qualify_table(
        style,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
    );
    let mut where_sql = Vec::new();
    for pk in pks {
        let v = delete.original.get(&pk.name).ok_or_else(|| {
            ConnectorError::Query(format!(
                "Cannot delete row: primary key “{}” missing from original row.",
                pk.name
            ))
        })?;
        if is_empty_value(v) {
            return Err(ConnectorError::Query(format!(
                "Cannot delete row: primary key “{}” is empty on the original row.",
                pk.name
            )));
        }
        where_sql.push(format!(
            "{} = {}",
            quote_ident(style, &pk.name),
            sql_literal(style, v, &pk.data_type)
        ));
    }
    Ok(format!(
        "DELETE FROM {target} WHERE {}",
        where_sql.join(" AND ")
    ))
}
