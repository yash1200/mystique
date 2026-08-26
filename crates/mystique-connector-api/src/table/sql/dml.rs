//! Compile grid edits into dialect-aware INSERT / UPDATE / DELETE / UPSERT.

use serde_json::Value as JsonValue;

use super::super::edits::{ApplyTableEditsInput, TableRowDelete, TableRowInsert, TableRowUpdate};
use super::{
    col_type, is_empty_value, pk_columns, qualify_table, quote_ident, sql_literal, SqlIdentStyle,
};
use crate::error::{ConnectorError, ConnectorResult};

pub fn compile_update(
    style: SqlIdentStyle,
    input: &ApplyTableEditsInput,
    update: &TableRowUpdate,
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
    insert: &TableRowInsert,
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
    insert: &TableRowInsert,
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
            Ok(format!(
                "{insert_sql} ON DUPLICATE KEY UPDATE {}",
                sets.join(", ")
            ))
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
    delete: &TableRowDelete,
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
