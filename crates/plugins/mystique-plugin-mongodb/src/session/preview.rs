//! Paged collection preview (sort, filter, count).

use std::time::Instant;

use mongodb::bson::{doc, Bson, Document};
use mystique_connector_api::{
    preview_sort_field, ColumnInfo, ConnectorError, ConnectorResult, FetchTablePreviewInput,
    TablePreviewResult,
};

use super::catalog;
use super::encode::bson_to_json;
use super::MongoSession;
use crate::preview_filter::compile_mongo_preview_predicate;

pub(super) async fn fetch_table_preview(
    session: &MongoSession,
    input: &FetchTablePreviewInput,
) -> ConnectorResult<TablePreviewResult> {
    session.ensure_open()?;
    let start = Instant::now();
    let db_name = session.resolve_database(input.database.as_deref())?;
    let columns = catalog::describe_table(session, Some(&db_name), None, &input.table).await?;
    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>(&input.table);
    let filter = compile_mongo_preview_predicate(&input.filters, input.code_filter.as_deref())?;
    let total_rows = if filter.is_empty() {
        coll.estimated_document_count().await.unwrap_or(0)
    } else {
        coll.count_documents(filter.clone())
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?
    };
    let sort = match preview_sort_field(
        &columns,
        input.sort_column.as_deref(),
        input.sort_dir.as_deref(),
    ) {
        Some((field, dir)) => doc! { field: dir },
        None => doc! { "_id": 1 },
    };
    let mut find = coll.find(filter).sort(sort);
    if input.offset > 0 {
        find = find.skip(input.offset as u64);
    }
    if input.max_rows > 0 {
        find = find.limit(input.max_rows as i64);
    }
    let mut cursor = find
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?;

    let mut docs = Vec::new();
    while cursor
        .advance()
        .await
        .map_err(|e| ConnectorError::Query(e.to_string()))?
    {
        let d = cursor
            .deserialize_current()
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        docs.push(d);
        if docs.len() >= input.max_rows as usize {
            break;
        }
    }

    let key_set: Vec<String> = if columns.is_empty() {
        let mut keys = Vec::new();
        for d in &docs {
            for k in d.keys() {
                if !keys.iter().any(|x| x == k) {
                    keys.push(k.clone());
                }
            }
        }
        if let Some(i) = keys.iter().position(|k| k == "_id") {
            let id = keys.remove(i);
            keys.insert(0, id);
        }
        keys
    } else {
        columns.iter().map(|c| c.name.clone()).collect()
    };

    let mut rows = Vec::new();
    for d in &docs {
        let mut cells = Vec::with_capacity(key_set.len());
        for k in &key_set {
            let v = d.get(k).cloned().unwrap_or(Bson::Null);
            cells.push(bson_to_json(v));
        }
        rows.push(cells);
    }

    let preview_columns = if columns.is_empty() {
        key_set
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
            .collect()
    } else {
        columns
    };

    let row_count = rows.len() as u64;
    let truncated = input.offset as u64 + row_count < total_rows;
    Ok(TablePreviewResult {
        engine: "mongodb".into(),
        columns: preview_columns,
        rows,
        row_count,
        total_rows,
        truncated,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}
