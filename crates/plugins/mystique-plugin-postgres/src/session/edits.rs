//! Grid insert / update / delete compilation and execution.

use mystique_connector_api::{
    compile_delete, compile_insert, compile_update, ApplyTableEditsInput, ApplyTableEditsResult,
    ConnectorError, ConnectorResult, SqlIdentStyle,
};

use super::PostgresSession;

pub(super) async fn apply_table_edits(
    session: &PostgresSession,
    edits: &ApplyTableEditsInput,
) -> ConnectorResult<ApplyTableEditsResult> {
    session.ensure_open()?;
    let client = session.client().await?;

    let mut updated = 0u32;
    let mut inserted = 0u32;
    let mut deleted = 0u32;
    for u in &edits.updates {
        let sql = compile_update(SqlIdentStyle::Postgres, edits, u)?;
        client
            .batch_execute(&sql)
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        updated += 1;
    }
    for i in &edits.inserts {
        let sql = compile_insert(SqlIdentStyle::Postgres, edits, i)?;
        client
            .batch_execute(&sql)
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        inserted += 1;
    }
    for d in &edits.deletes {
        let sql = compile_delete(SqlIdentStyle::Postgres, edits, d)?;
        client
            .batch_execute(&sql)
            .await
            .map_err(|e| ConnectorError::Query(e.to_string()))?;
        deleted += 1;
    }
    Ok(ApplyTableEditsResult {
        updated,
        inserted,
        deleted,
    })
}
