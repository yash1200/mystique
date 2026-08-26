//! Export and import via the shared connector-api helpers.

use mystique_connector_api::{
    export_object_with_preview, import_chunk_sql, import_object_from_file,
    preview_import_with_describe, ConnectorError, ConnectorResult, ExportObjectInput,
    ExportObjectResult, ImportChunkInput, ImportChunkResult, ImportObjectInput, ImportObjectResult,
    PreviewImportInput, PreviewImportResult, SqlIdentStyle,
};

use super::PostgresSession;

pub(super) async fn export_object(
    session: &PostgresSession,
    input: &ExportObjectInput,
) -> ConnectorResult<ExportObjectResult> {
    session.ensure_open()?;
    export_object_with_preview(session, input, Some(SqlIdentStyle::Postgres)).await
}

pub(super) async fn preview_import(
    session: &PostgresSession,
    input: &PreviewImportInput,
) -> ConnectorResult<PreviewImportResult> {
    session.ensure_open()?;
    preview_import_with_describe(session, input).await
}

pub(super) async fn import_object(
    session: &PostgresSession,
    input: &ImportObjectInput,
) -> ConnectorResult<ImportObjectResult> {
    session.ensure_open()?;
    import_object_from_file(session, input).await
}

pub(super) async fn import_chunk(
    session: &PostgresSession,
    input: &ImportChunkInput,
) -> ConnectorResult<ImportChunkResult> {
    session.ensure_open()?;
    let (statements, counts) = import_chunk_sql(SqlIdentStyle::Postgres, input)?;
    if statements.is_empty() {
        return Ok(counts);
    }
    let client = session.client().await?;
    for sql in statements {
        client
            .batch_execute(&sql)
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
    }
    Ok(counts)
}
