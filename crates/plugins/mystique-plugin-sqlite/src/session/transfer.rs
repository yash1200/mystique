//! Export and import via the shared connector-api helpers.

use mystique_connector_api::{
    export_object_with_preview, import_chunk_sql, import_object_from_file,
    preview_import_with_describe, ConnectorError, ConnectorResult, ExportObjectInput,
    ExportObjectResult, ImportChunkInput, ImportChunkResult, ImportObjectInput, ImportObjectResult,
    PreviewImportInput, PreviewImportResult,
};
use mystique_sql::SqlDialect;

use super::query_failed;
use super::SqliteSession;

pub(super) async fn export_object(
    session: &SqliteSession,
    input: &ExportObjectInput,
) -> ConnectorResult<ExportObjectResult> {
    session.ensure_open()?;
    export_object_with_preview(session, input, Some(SqlDialect::Sqlite)).await
}

pub(super) async fn preview_import(
    session: &SqliteSession,
    input: &PreviewImportInput,
) -> ConnectorResult<PreviewImportResult> {
    session.ensure_open()?;
    preview_import_with_describe(session, input).await
}

pub(super) async fn import_object(
    session: &SqliteSession,
    input: &ImportObjectInput,
) -> ConnectorResult<ImportObjectResult> {
    session.ensure_open()?;
    import_object_from_file(session, input).await
}

pub(super) async fn import_chunk(
    session: &SqliteSession,
    input: &ImportChunkInput,
) -> ConnectorResult<ImportChunkResult> {
    session.ensure_open()?;
    let (statements, counts) = import_chunk_sql(SqlDialect::Sqlite, input)?;
    if statements.is_empty() {
        return Ok(counts);
    }
    let pool = session.pool();
    session
        .blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            for sql in statements {
                conn.execute(&sql, []).map_err(query_failed)?;
            }
            Ok(counts)
        })
        .await
}
