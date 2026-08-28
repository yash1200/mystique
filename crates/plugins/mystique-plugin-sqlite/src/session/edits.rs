//! Grid insert / update / delete compilation and execution.

use mystique_connector_api::{
    compile_delete, compile_insert, compile_update, ApplyTableEditsInput, ApplyTableEditsResult,
    ConnectorError, ConnectorResult, SqlIdentStyle,
};

use super::query_failed;
use super::SqliteSession;

pub(super) async fn apply_table_edits(
    session: &SqliteSession,
    edits: &ApplyTableEditsInput,
) -> ConnectorResult<ApplyTableEditsResult> {
    session.ensure_open()?;
    let mut statements = Vec::new();
    for u in &edits.updates {
        statements.push(compile_update(SqlIdentStyle::Postgres, edits, u)?);
    }
    let update_n = statements.len();
    for i in &edits.inserts {
        statements.push(compile_insert(SqlIdentStyle::Postgres, edits, i)?);
    }
    let insert_n = statements.len() - update_n;
    for d in &edits.deletes {
        statements.push(compile_delete(SqlIdentStyle::Postgres, edits, d)?);
    }
    let delete_n = statements.len() - update_n - insert_n;
    let pool = session.pool();
    session
        .blocking(move || -> ConnectorResult<ApplyTableEditsResult> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            for sql in &statements {
                conn.execute(sql.as_str(), []).map_err(query_failed)?;
            }
            Ok(ApplyTableEditsResult {
                updated: update_n as u32,
                inserted: insert_n as u32,
                deleted: delete_n as u32,
            })
        })
        .await
}
