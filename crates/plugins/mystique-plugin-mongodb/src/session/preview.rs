//! Paged collection preview (sort, filter, count).

use std::time::{Duration, Instant};

use mongodb::bson::{doc, Bson, Document};
use mystique_connector_api::{
    ColumnInfo, ConnectorError, ConnectorResult, FetchTablePreviewInput, QueryControl,
    TablePreviewResult, TablePreviewStats, INACTIVITY_TIMEOUT_MS, MAX_TIMEOUT_MS,
};

use super::encode::bson_to_json;
use super::query_failed;
use super::MongoSession;

async fn mongo_drive<T, F>(control: &QueryControl, action: F) -> ConnectorResult<T>
where
    F: std::future::IntoFuture<Output = Result<T, mongodb::error::Error>>,
{
    let fut = action.into_future();
    tokio::select! {
        biased;
        _ = control.token.cancelled() => Err(ConnectorError::Query("Query cancelled.".into())),
        result = fut => result.map_err(query_failed),
    }
}
use crate::preview_filter::compile_mongo_preview_predicate;

pub(super) async fn fetch_table_preview(
    session: &MongoSession,
    input: &FetchTablePreviewInput,
) -> ConnectorResult<TablePreviewResult> {
    session.ensure_open()?;
    let start = Instant::now();
    let db_name = session.resolve_database(input.database.as_deref())?;
    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(&input.table);
    let filter = compile_mongo_preview_predicate(&input.filters, input.code_filter.as_deref())?;
    let sort = match input.sort_column.as_deref().filter(|s| !s.is_empty()) {
        Some(col) => {
            let dir = if input.sort_dir.as_deref() == Some("desc") {
                -1
            } else {
                1
            };
            doc! { col: dir }
        }
        None => doc! { "_id": 1 },
    };
    let mut find = coll.find(filter).sort(sort);
    if input.offset > 0 {
        find = find.skip(input.offset as u64);
    }
    if input.max_rows > 0 {
        find = find.limit(input.max_rows as i64);
    }
    // find with maxTimeMS = the global cap. Dropping the cursor on cancel aborts it.
    let find = find.max_time(Duration::from_millis(MAX_TIMEOUT_MS));
    let mut cursor = mongo_drive(&input.control, find).await?;

    let mut docs = Vec::new();
    let mut saw_doc = false;
    loop {
        let advanced = tokio::select! {
            biased;
            _ = input.control.token.cancelled() => {
                drop(cursor);
                return Err(ConnectorError::Query("Query cancelled.".into()));
            }
            _ = tokio::time::sleep(Duration::from_millis(INACTIVITY_TIMEOUT_MS)), if saw_doc => {
                input.control.cancel();
                drop(cursor);
                return Err(ConnectorError::Timeout(format!(
                    "No rows arrived for {INACTIVITY_TIMEOUT_MS} ms."
                )));
            }
            advanced = cursor.advance() => advanced.map_err(query_failed)?,
        };
        if !advanced {
            break;
        }
        saw_doc = true;
        let d = cursor.deserialize_current().map_err(query_failed)?;
        docs.push(d);
        if docs.len() >= input.max_rows as usize {
            break;
        }
    }

    let mut key_set: Vec<String> = Vec::new();
    for d in &docs {
        for k in d.keys() {
            if !key_set.iter().any(|x| x == k) {
                key_set.push(k.clone());
            }
        }
    }
    if let Some(i) = key_set.iter().position(|k| k == "_id") {
        let id = key_set.remove(i);
        key_set.insert(0, id);
    }

    let mut rows = Vec::new();
    for d in &docs {
        let mut cells = Vec::with_capacity(key_set.len());
        for k in &key_set {
            let v = d.get(k).cloned().unwrap_or(Bson::Null);
            cells.push(bson_to_json(v));
        }
        rows.push(cells);
    }

    let preview_columns: Vec<ColumnInfo> = key_set
        .iter()
        .map(|k| ColumnInfo {
            name: k.clone(),
            data_type: "mixed".into(),
            nullable: k != "_id",
            pk: k == "_id",
            enum_values: None,
            default_value: None,
            references: None,
        })
        .collect();

    let row_count = rows.len() as u64;
    let truncated = row_count >= u64::from(input.max_rows);
    Ok(TablePreviewResult {
        engine: "mongodb".into(),
        columns: preview_columns,
        rows,
        row_count,
        total_rows: 0,
        total_known: false,
        truncated,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

pub(super) async fn fetch_table_preview_stats(
    session: &MongoSession,
    input: &FetchTablePreviewInput,
) -> ConnectorResult<TablePreviewStats> {
    session.ensure_open()?;
    let db_name = session.resolve_database(input.database.as_deref())?;
    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(&input.table);
    let filter = compile_mongo_preview_predicate(&input.filters, input.code_filter.as_deref())?;
    let total_rows = if filter.is_empty() {
        coll.estimated_document_count().await.unwrap_or(0)
    } else {
        coll.count_documents(filter).await.map_err(query_failed)?
    };
    Ok(TablePreviewStats {
        total_rows,
        total_known: true,
        references: Vec::new(),
    })
}
