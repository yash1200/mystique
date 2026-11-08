//! Preview, remap, and chunked import execution.

use std::collections::BTreeMap;

use serde_json::Value as JsonValue;

use mystique_sql::dml::{compile_insert, compile_upsert};
use mystique_transfer::{parse_import_file, suggest_map};

use super::super::edits::{ApplyTableEditsInput, TableEditColumn, TableRowInsert};
use super::{
    ImportChunkInput, ImportChunkResult, ImportColumnMap, ImportObjectInput, ImportObjectResult,
    ImportOnDuplicate, ImportTableColumn, PreviewImportInput, PreviewImportResult,
    IMPORT_CHUNK_SIZE, IMPORT_PREVIEW_ROWS,
};
use crate::catalog::ColumnInfo;
use crate::connector::ConnectorSession;
use crate::error::{ConnectorError, ConnectorResult};
use mystique_sql::SqlDialect;

pub async fn preview_import_with_describe(
    session: &dyn ConnectorSession,
    input: &PreviewImportInput,
) -> ConnectorResult<PreviewImportResult> {
    if input.path.trim().is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "Import path is required.".into(),
        ));
    }
    let columns = session
        .describe_table(
            input.database.as_deref(),
            input.schema.as_deref(),
            &input.name,
        )
        .await?;
    let parsed = parse_import_file(
        &input.path,
        input.format,
        input.encoding,
        Some(IMPORT_PREVIEW_ROWS),
    )?;
    let suggested_map = suggest_map(&parsed.keys, &column_names(&columns));
    Ok(PreviewImportResult {
        file_keys: parsed.keys,
        columns: columns
            .into_iter()
            .map(|c| ImportTableColumn {
                name: c.name,
                data_type: c.data_type,
                pk: c.pk,
                nullable: c.nullable,
            })
            .collect(),
        suggested_map,
        rows: parsed.rows,
        format: input.format,
        encoding: input.encoding,
    })
}

fn column_names(columns: &[ColumnInfo]) -> Vec<&str> {
    columns.iter().map(|c| c.name.as_str()).collect()
}

fn remap_row(
    keys: &[String],
    row: &[JsonValue],
    mapping: &[ImportColumnMap],
) -> BTreeMap<String, JsonValue> {
    let mut out = BTreeMap::new();
    for map in mapping {
        let Some(col) = map
            .table_column
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let idx = keys.iter().position(|k| k == &map.file_key);
        let val = idx
            .and_then(|i| row.get(i))
            .cloned()
            .unwrap_or(JsonValue::Null);
        out.insert(col.to_string(), val);
    }
    out
}

pub async fn import_object_from_file(
    session: &dyn ConnectorSession,
    input: &ImportObjectInput,
) -> ConnectorResult<ImportObjectResult> {
    if input.path.trim().is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "Import path is required.".into(),
        ));
    }
    if input.name.trim().is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "Object name is required.".into(),
        ));
    }
    let columns = session
        .describe_table(
            input.database.as_deref(),
            input.schema.as_deref(),
            &input.name,
        )
        .await?;
    if columns.is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "Target object has no columns.".into(),
        ));
    }
    let parsed = parse_import_file(&input.path, input.format, input.encoding, None)?;
    let mapping = if input.mapping.is_empty() {
        suggest_map(&parsed.keys, &column_names(&columns))
    } else {
        input.mapping.clone()
    };
    if mapping.iter().all(|m| {
        m.table_column
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    }) {
        return Err(ConnectorError::InvalidConfig(
            "Map at least one file field to a column.".into(),
        ));
    }

    let mut inserted = 0u64;
    let mut updated = 0u64;
    let mut chunk: Vec<BTreeMap<String, JsonValue>> = Vec::with_capacity(IMPORT_CHUNK_SIZE);
    for row in &parsed.rows {
        let mapped = remap_row(&parsed.keys, row, &mapping);
        if mapped.is_empty() {
            continue;
        }
        chunk.push(mapped);
        if chunk.len() >= IMPORT_CHUNK_SIZE {
            let result = session
                .import_chunk(&ImportChunkInput {
                    name: input.name.clone(),
                    schema: input.schema.clone(),
                    database: input.database.clone(),
                    columns: columns.clone(),
                    rows: std::mem::take(&mut chunk),
                    on_duplicate: input.on_duplicate,
                })
                .await?;
            inserted += result.inserted;
            updated += result.updated;
        }
    }
    if !chunk.is_empty() {
        let result = session
            .import_chunk(&ImportChunkInput {
                name: input.name.clone(),
                schema: input.schema.clone(),
                database: input.database.clone(),
                columns: columns.clone(),
                rows: chunk,
                on_duplicate: input.on_duplicate,
            })
            .await?;
        inserted += result.inserted;
        updated += result.updated;
    }

    Ok(ImportObjectResult {
        inserted,
        updated,
        format: input.format,
        message: Some(format!(
            "Imported {} row(s) ({} inserted, {} updated).",
            inserted + updated,
            inserted,
            updated
        )),
    })
}

/// Shared SQL write for one import chunk (insert or upsert).
pub fn import_chunk_sql(
    style: SqlDialect,
    chunk: &ImportChunkInput,
) -> ConnectorResult<(Vec<String>, ImportChunkResult)> {
    let edits = ApplyTableEditsInput {
        table: chunk.name.clone(),
        schema: chunk.schema.clone(),
        database: chunk.database.clone(),
        columns: chunk
            .columns
            .iter()
            .map(|c| TableEditColumn {
                name: c.name.clone(),
                data_type: c.data_type.clone(),
                nullable: c.nullable,
                pk: c.pk,
            })
            .collect(),
        updates: Vec::new(),
        inserts: Vec::new(),
        deletes: Vec::new(),
        dry_run: false,
    };
    let mut statements = Vec::new();
    let mut result = ImportChunkResult::default();
    for row in &chunk.rows {
        let insert = TableRowInsert {
            values: row.clone(),
        };
        match chunk.on_duplicate {
            ImportOnDuplicate::StopOnError => {
                statements.push(compile_insert(style, &edits, &insert)?);
                result.inserted += 1;
            }
            ImportOnDuplicate::UpdateOnDuplicate => {
                statements.push(compile_upsert(style, &edits, &insert)?);
                result.updated += 1;
            }
        }
    }
    Ok((statements, result))
}
