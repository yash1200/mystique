//! Preview, remap, and chunked import execution.

use std::collections::BTreeMap;

use serde_json::Value as JsonValue;

use mystique_sql::dml::{compile_insert, compile_upsert};
use mystique_transfer::{parse_import_file, stream_import_file, suggest_map, ImportRecord};

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

fn remap_record(record: &ImportRecord, mapping: &[ImportColumnMap]) -> BTreeMap<String, JsonValue> {
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
        let val = record
            .get(&map.file_key)
            .cloned()
            .unwrap_or(JsonValue::Null);
        out.insert(col.to_string(), val);
    }
    out
}

/// Records parsed per batch handed from the reader thread.
const PARSE_BATCH: usize = IMPORT_CHUNK_SIZE;
/// Parsed batches the reader may run ahead of the writes.
const PARSE_QUEUE: usize = 2;

/// Parse the file on a reader thread and write it in [`IMPORT_CHUNK_SIZE`]
/// batches as it is read. Memory is a few batches whatever the file size.
/// Each batch is one write; a cancel or a failure stops before the next one
/// and says how many rows were already written (they are kept).
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

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<ImportRecord>>(PARSE_QUEUE);
    let path = input.path.clone();
    let format = input.format;
    let encoding = input.encoding;
    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
    let spawned = std::thread::Builder::new()
        .name("mystique-import".into())
        .spawn(move || {
            let parsed = stream_import_file(&path, format, encoding, PARSE_BATCH, &mut |batch| {
                tx.blocking_send(batch).is_ok()
            });
            let _ = done_tx.send(parsed);
        });
    if let Err(err) = spawned {
        return Err(ConnectorError::Internal(format!(
            "could not start the import reader: {err}"
        )));
    }

    let mut mapping = input.mapping.clone();
    let mut inserted = 0u64;
    let mut updated = 0u64;
    let written = |inserted: u64, updated: u64| inserted + updated;
    let stopped = |rows: u64, why: String| -> ConnectorError {
        if rows == 0 {
            ConnectorError::Query(why)
        } else {
            ConnectorError::Query(format!(
                "{why} {rows} row(s) were already imported and are kept."
            ))
        }
    };
    while let Some(records) = rx.recv().await {
        if input.control.is_cancelled() {
            return Err(stopped(
                written(inserted, updated),
                "Import cancelled.".into(),
            ));
        }
        if mapping.is_empty() {
            let keys: Vec<String> = records
                .first()
                .map(|r| r.keys().cloned().collect())
                .unwrap_or_default();
            mapping = suggest_map(&keys, &column_names(&columns));
        }
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
        let rows: Vec<BTreeMap<String, JsonValue>> = records
            .iter()
            .map(|record| remap_record(record, &mapping))
            .filter(|row| !row.is_empty())
            .collect();
        drop(records);
        for chunk in rows.chunks(IMPORT_CHUNK_SIZE) {
            let result = session
                .import_chunk(&ImportChunkInput {
                    name: input.name.clone(),
                    schema: input.schema.clone(),
                    database: input.database.clone(),
                    columns: columns.clone(),
                    rows: chunk.to_vec(),
                    on_duplicate: input.on_duplicate,
                })
                .await
                .map_err(|err| stopped(written(inserted, updated), err.user_message()))?;
            inserted += result.inserted;
            updated += result.updated;
            if let Some(progress) = &input.progress {
                (progress.0)(written(inserted, updated));
            }
        }
    }
    // The reader's own error (a bad row, an unreadable file).
    match done_rx.await {
        Ok(Ok(_)) => {}
        Ok(Err(err)) => {
            return Err(stopped(
                written(inserted, updated),
                ConnectorError::from(err).user_message(),
            ))
        }
        Err(_) => {
            return Err(ConnectorError::Internal(
                "the import reader stopped without a result".into(),
            ))
        }
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
