//! Export and import via the shared connector-api helpers.

use mysql_async::prelude::*;
use mystique_connector_api::{
    export_object_with_preview, import_chunk_sql, import_object_from_file,
    preview_import_with_describe, ConnectorResult, ExportObjectInput, ExportObjectResult,
    ImportChunkInput, ImportChunkResult, ImportObjectInput, ImportObjectResult, PreviewImportInput,
    PreviewImportResult,
};
use mystique_sql::SqlDialect;

use super::query_failed;
use super::MysqlSession;

pub(super) async fn export_object(
    session: &MysqlSession,
    input: &ExportObjectInput,
) -> ConnectorResult<ExportObjectResult> {
    session.ensure_open()?;
    export_object_with_preview(session, input, Some(SqlDialect::Mysql)).await
}

pub(super) async fn preview_import(
    session: &MysqlSession,
    input: &PreviewImportInput,
) -> ConnectorResult<PreviewImportResult> {
    session.ensure_open()?;
    preview_import_with_describe(session, input).await
}

pub(super) async fn import_object(
    session: &MysqlSession,
    input: &ImportObjectInput,
) -> ConnectorResult<ImportObjectResult> {
    session.ensure_open()?;
    import_object_from_file(session, input).await
}

pub(super) async fn import_chunk(
    session: &MysqlSession,
    input: &ImportChunkInput,
) -> ConnectorResult<ImportChunkResult> {
    session.ensure_open()?;
    let (statements, counts) = import_chunk_sql(SqlDialect::Mysql, input)?;
    if statements.is_empty() {
        return Ok(counts);
    }
    let mut conn = session.conn().await?;
    for sql in statements {
        conn.query_drop(sql).await.map_err(query_failed)?;
    }
    Ok(counts)
}
