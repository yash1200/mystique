//! Format-specific file writers for table export.

use std::fs::File;
use std::io::{BufWriter, Write};

use serde_json::{Map, Value as JsonValue};

use super::super::sql::{qualify_table, quote_ident, sql_literal, SqlIdentStyle};
use crate::catalog::ColumnInfo;
use crate::error::{ConnectorError, ConnectorResult};

pub(super) fn write_io(out: &mut BufWriter<File>, bytes: &[u8]) -> ConnectorResult<()> {
    out.write_all(bytes)
        .map_err(|e| ConnectorError::Internal(format!("export write failed: {e}")))
}

pub(super) fn writeln_io(out: &mut BufWriter<File>, s: &str) -> ConnectorResult<()> {
    write_io(out, s.as_bytes())
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// File export keeps the full cell. The agent/MCP window is a different path.
fn cell_text(v: &JsonValue) -> String {
    match v {
        JsonValue::Null => String::new(),
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

pub(super) fn write_csv_header(
    out: &mut BufWriter<File>,
    columns: &[ColumnInfo],
) -> ConnectorResult<()> {
    let line = columns
        .iter()
        .map(|c| csv_escape(&c.name))
        .collect::<Vec<_>>()
        .join(",");
    writeln_io(out, &format!("{line}\n"))
}

pub(super) fn write_csv_rows(
    out: &mut BufWriter<File>,
    columns: &[ColumnInfo],
    rows: &[Vec<JsonValue>],
) -> ConnectorResult<()> {
    for row in rows {
        let line = columns
            .iter()
            .enumerate()
            .map(|(i, _)| csv_escape(&cell_text(row.get(i).unwrap_or(&JsonValue::Null))))
            .collect::<Vec<_>>()
            .join(",");
        writeln_io(out, &format!("{line}\n"))?;
    }
    Ok(())
}

fn row_object(columns: &[ColumnInfo], row: &[JsonValue]) -> JsonValue {
    let mut map = Map::new();
    for (i, col) in columns.iter().enumerate() {
        map.insert(
            col.name.clone(),
            row.get(i).cloned().unwrap_or(JsonValue::Null),
        );
    }
    JsonValue::Object(map)
}

pub(super) fn write_json_array_rows(
    out: &mut BufWriter<File>,
    columns: &[ColumnInfo],
    rows: &[Vec<JsonValue>],
    already_wrote: bool,
) -> ConnectorResult<()> {
    for (i, row) in rows.iter().enumerate() {
        if already_wrote || i > 0 {
            write_io(out, b",\n")?;
        }
        let pretty = serde_json::to_string_pretty(&row_object(columns, row))
            .map_err(|e| ConnectorError::Internal(format!("json encode: {e}")))?;
        let indented = pretty
            .lines()
            .map(|l| format!("  {l}"))
            .collect::<Vec<_>>()
            .join("\n");
        writeln_io(out, &format!("{indented}\n"))?;
    }
    Ok(())
}

pub(super) fn write_jsonl_rows(
    out: &mut BufWriter<File>,
    columns: &[ColumnInfo],
    rows: &[Vec<JsonValue>],
) -> ConnectorResult<()> {
    for row in rows {
        let line = serde_json::to_string(&row_object(columns, row))
            .map_err(|e| ConnectorError::Internal(format!("json encode: {e}")))?;
        writeln_io(out, &format!("{line}\n"))?;
    }
    Ok(())
}

pub(super) fn write_sql_insert(
    out: &mut BufWriter<File>,
    style: SqlIdentStyle,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    columns: &[ColumnInfo],
    rows: &[Vec<JsonValue>],
) -> ConnectorResult<()> {
    if rows.is_empty() || columns.is_empty() {
        return Ok(());
    }
    let target = qualify_table(style, database, schema, table);
    let col_list = columns
        .iter()
        .map(|c| quote_ident(style, &c.name))
        .collect::<Vec<_>>()
        .join(", ");
    let values = rows
        .iter()
        .map(|row| {
            let cells = columns
                .iter()
                .enumerate()
                .map(|(i, col)| {
                    sql_literal(
                        style,
                        row.get(i).unwrap_or(&JsonValue::Null),
                        &col.data_type,
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("({cells})")
        })
        .collect::<Vec<_>>()
        .join(",\n  ");
    writeln_io(
        out,
        &format!("INSERT INTO {target} ({col_list}) VALUES\n  {values};\n\n"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ColumnInfo;

    fn doc_column() -> ColumnInfo {
        ColumnInfo {
            name: "doc".into(),
            data_type: "json".into(),
            nullable: true,
            pk: false,
            enum_values: None,
            default_value: None,
            references: None,
        }
    }

    #[test]
    fn file_export_keeps_a_long_json_cell() {
        let cell = JsonValue::String("x".repeat(20_000));
        assert_eq!(cell_text(&cell).chars().count(), 20_000);
        let row = row_object(&[doc_column()], &[cell]);
        assert_eq!(row["doc"].as_str().map(|s| s.chars().count()), Some(20_000));
    }
}
