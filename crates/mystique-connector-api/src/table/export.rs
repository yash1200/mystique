//! Plugin-owned table/collection file export.
//!
//! Plugins fetch in [`EXPORT_CHUNK_SIZE`] pages and append to the destination
//! file until the engine reports no more rows. The FE only picks path + format.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};

use crate::catalog::{ColumnInfo, FetchTablePreviewInput};
use crate::connector::ConnectorSession;
use crate::error::{ConnectorError, ConnectorResult};
use super::sql::{qualify_table, quote_ident, sql_literal, SqlIdentStyle};

/// Rows written per engine fetch / file append.
pub const EXPORT_CHUNK_SIZE: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Csv,
    Json,
    Jsonl,
    Sql,
}

impl ExportFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Jsonl => "jsonl",
            Self::Sql => "sql",
        }
    }
}

/// IPC request. `path` is the absolute destination file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportObjectRequest {
    pub session_id: String,
    pub name: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    pub format: ExportFormat,
    pub path: String,
}

/// Validated payload handed to the plugin.
#[derive(Debug, Clone)]
pub struct ExportObjectInput {
    pub name: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub kind: Option<String>,
    pub format: ExportFormat,
    pub path: String,
}

impl ExportObjectRequest {
    pub fn into_input(self) -> ExportObjectInput {
        ExportObjectInput {
            name: self.name,
            schema: self.schema,
            database: self.database,
            kind: self.kind,
            format: self.format,
            path: self.path,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportObjectResult {
    pub path: String,
    pub row_count: u64,
    pub format: ExportFormat,
}

/// Fetch 100-row pages via the plugin preview path and stream them to `input.path`.
pub async fn export_object_with_preview(
    session: &dyn ConnectorSession,
    input: &ExportObjectInput,
    sql_style: Option<SqlIdentStyle>,
) -> ConnectorResult<ExportObjectResult> {
    if matches!(input.format, ExportFormat::Sql) && sql_style.is_none() {
        return Err(ConnectorError::NotSupported(
            "SQL export is not available for this engine.".into(),
        ));
    }
    if input.path.trim().is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "Export path is required.".into(),
        ));
    }
    if input.name.trim().is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "Object name is required.".into(),
        ));
    }

    let path = Path::new(&input.path);
    let file = File::create(path)
        .map_err(|e| ConnectorError::Internal(format!("could not create {}: {e}", input.path)))?;
    let mut out = BufWriter::new(file);

    let mut offset: u32 = 0;
    let mut row_count: u64 = 0;
    let mut columns: Vec<ColumnInfo> = Vec::new();
    let mut json_started = false;

    if matches!(input.format, ExportFormat::Json) {
        write_io(&mut out, b"[\n")?;
    }

    loop {
        let page = session
            .fetch_table_preview(&FetchTablePreviewInput {
                table: input.name.clone(),
                schema: input.schema.clone(),
                database: input.database.clone(),
                max_rows: EXPORT_CHUNK_SIZE,
                offset,
                filters: Vec::new(),
                code_filter: None,
                sort_column: None,
                sort_dir: None,
            })
            .await?;

        if columns.is_empty() && !page.columns.is_empty() {
            columns = page.columns;
            if matches!(input.format, ExportFormat::Csv) {
                write_csv_header(&mut out, &columns)?;
            }
        }

        if page.rows.is_empty() {
            break;
        }

        match input.format {
            ExportFormat::Csv => write_csv_rows(&mut out, &columns, &page.rows)?,
            ExportFormat::Json => {
                write_json_array_rows(&mut out, &columns, &page.rows, json_started)?;
                json_started = true;
            }
            ExportFormat::Jsonl => write_jsonl_rows(&mut out, &columns, &page.rows)?,
            ExportFormat::Sql => {
                let style = sql_style.expect("checked above");
                write_sql_insert(
                    &mut out,
                    style,
                    input.database.as_deref(),
                    input.schema.as_deref(),
                    &input.name,
                    &columns,
                    &page.rows,
                )?;
            }
        }

        row_count += page.rows.len() as u64;
        if page.rows.len() < EXPORT_CHUNK_SIZE as usize {
            break;
        }
        let next = offset.saturating_add(EXPORT_CHUNK_SIZE);
        if next <= offset {
            break;
        }
        offset = next;
    }

    match input.format {
        ExportFormat::Json => write_io(&mut out, b"]\n")?,
        ExportFormat::Csv if columns.is_empty() => write_io(&mut out, b"\n")?,
        ExportFormat::Sql if row_count == 0 => {
            writeln_io(
                &mut out,
                &format!("-- No rows to export for {}\n", input.name),
            )?;
        }
        _ => {}
    }

    out.flush()
        .map_err(|e| ConnectorError::Internal(format!("could not write {}: {e}", input.path)))?;

    Ok(ExportObjectResult {
        path: input.path.clone(),
        row_count,
        format: input.format,
    })
}

fn write_io(out: &mut BufWriter<File>, bytes: &[u8]) -> ConnectorResult<()> {
    out.write_all(bytes)
        .map_err(|e| ConnectorError::Internal(format!("export write failed: {e}")))
}

fn writeln_io(out: &mut BufWriter<File>, s: &str) -> ConnectorResult<()> {
    write_io(out, s.as_bytes())
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn cell_text(v: &JsonValue) -> String {
    match v {
        JsonValue::Null => String::new(),
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

fn write_csv_header(out: &mut BufWriter<File>, columns: &[ColumnInfo]) -> ConnectorResult<()> {
    let line = columns
        .iter()
        .map(|c| csv_escape(&c.name))
        .collect::<Vec<_>>()
        .join(",");
    writeln_io(out, &format!("{line}\n"))
}

fn write_csv_rows(
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

fn write_json_array_rows(
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

fn write_jsonl_rows(
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

fn write_sql_insert(
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
