//! Grid save rollback by backend `save_id`.
//!
//! The plugin builds the inverse SQL and post-save row hashes inside the save
//! transaction. The manager stores them under a new `save_id`. Rollback takes
//! only that id: it is claimed (deleted) before the inverse runs and put back
//! only when the engine refused before commit for a reason other than stale rows.

use std::sync::Arc;

use mystique_connector_api::{
    ApplyTableEditsResult, ConnectorError, ConnectorResult, RollbackTableEditsRequest, WriteAction,
    WriteRisk, ROLLBACK_STALE,
};
use uuid::Uuid;

use crate::domain::{grid_table_key, now_iso, GridSaveRecord};
use crate::error::AppResult;

use super::guard::WriteCheck;
use super::ConnectionManager;

pub(crate) trait GridSaveStore: Send + Sync {
    fn put(&self, save: &GridSaveRecord) -> AppResult<()>;
    fn get(&self, save_id: &str) -> AppResult<Option<GridSaveRecord>>;
    /// Delete and return. A second take of the same id is `None`.
    fn take(&self, save_id: &str) -> AppResult<Option<GridSaveRecord>>;
}

impl ConnectionManager {
    pub(crate) fn set_grid_save_store(&self, store: Arc<dyn GridSaveStore>) {
        if let Ok(mut slot) = self.grid_saves.lock() {
            *slot = Some(store);
        }
    }

    fn grid_save_store(&self) -> Option<Arc<dyn GridSaveStore>> {
        self.grid_saves.lock().ok().and_then(|slot| slot.clone())
    }

    /// Saves belong to the profile, so a reconnect keeps them.
    async fn save_owner(&self, session_id: &str) -> ConnectorResult<String> {
        let guard = self.sessions.read().await;
        let live = guard.get(session_id).ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("session not found: {session_id}"))
        })?;
        Ok(live
            .profile_id
            .clone()
            .unwrap_or_else(|| session_id.to_string()))
    }

    /// Stores the plugin's undo and sets `save_id`. No store or no undo: no id.
    pub(super) async fn remember_save(
        &self,
        session_id: &str,
        database: Option<&str>,
        schema: Option<&str>,
        table: &str,
        result: &mut ApplyTableEditsResult,
    ) {
        let Some(undo) = result.undo.take() else {
            return;
        };
        let (Some(store), Ok(connection_id)) =
            (self.grid_save_store(), self.save_owner(session_id).await)
        else {
            return;
        };
        let record = GridSaveRecord {
            save_id: Uuid::new_v4().to_string(),
            connection_id,
            table_key: grid_table_key(database, schema, table),
            undo,
            created_at: now_iso(),
        };
        let save_id = record.save_id.clone();
        let stored = blocking(move || store.put(&record)).await;
        if stored.is_ok() {
            result.save_id = Some(save_id);
        }
    }

    /// Roll back one save. Single use, refused when the rows changed since.
    pub async fn rollback_table_edits(
        &self,
        req: RollbackTableEditsRequest,
    ) -> ConnectorResult<()> {
        let store = self.grid_save_store().ok_or_else(|| {
            ConnectorError::NotSupported("Rollback history is not available.".into())
        })?;
        let owner = self.save_owner(&req.session_id).await?;
        let lookup = Arc::clone(&store);
        let save_id = req.save_id.clone();
        let save = blocking(move || lookup.get(&save_id))
            .await?
            .ok_or_else(already_used)?;
        if save.connection_id != owner {
            return Err(ConnectorError::Policy(
                "This save belongs to another connection.".into(),
            ));
        }
        self.enforce_write_guard(WriteCheck {
            preview: "Roll back a grid save".into(),
            statements: save.undo.inverse.clone(),
            confirm_object_name: req.confirm_object_name.as_deref(),
            ..WriteCheck::new(&req.session_id, WriteAction::GridSave, WriteRisk::Write)
        })
        .await?;
        let session = self.session(&req.session_id).await?;
        let claim = Arc::clone(&store);
        let save_id = req.save_id.clone();
        let save = blocking(move || claim.take(&save_id))
            .await?
            .ok_or_else(already_used)?;
        let started = std::time::Instant::now();
        let outcome = session.apply_grid_undo(&save.undo).await;
        self.log_action(super::history::HistoryAction {
            session_id: &req.session_id,
            kind: crate::domain::QueryHistoryKind::Rollback,
            sql: save.undo.inverse.clone(),
            database: None,
            started,
            outcome: outcome.as_ref().map(|()| None),
        })
        .await;
        if let Err(err) = &outcome {
            if !err.to_string().contains(ROLLBACK_STALE) {
                let _ = blocking(move || store.put(&save)).await;
            }
        }
        outcome
    }
}

fn already_used() -> ConnectorError {
    ConnectorError::Policy("This save was already rolled back or is no longer available.".into())
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> ConnectorResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|err| ConnectorError::Internal(err.to_string()))?
        .map_err(|err| ConnectorError::Internal(err.to_string()))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use mystique_connector_api::{
        ApplyTableEditsRequest, ConnectorError, RollbackTableEditsRequest, TableEditColumn,
        TableRowUpdate,
    };
    use serde_json::json;

    use super::super::{ConnectionManager, OpenSessionRequest};
    use crate::application::DbGridSaveStore;
    use crate::storage::AppDatabase;

    const SEED: &str =
        "CREATE TABLE docs (id INTEGER PRIMARY KEY, n INTEGER); INSERT INTO docs VALUES (1, 10);";

    fn temp_db(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "mystique-grid-save-{label}-{}-{nanos}.sqlite",
            std::process::id()
        ))
    }

    fn sql(path: &Path, text: &str) {
        let conn = rusqlite::Connection::open(path).expect("open file");
        conn.execute_batch(text).expect("sql");
    }

    fn n_of(path: &Path) -> i64 {
        let conn = rusqlite::Connection::open(path).expect("open file");
        conn.query_row("SELECT n FROM docs WHERE id = 1", [], |row| row.get(0))
            .expect("read")
    }

    async fn manager(path: &Path, session_id: &str, environment: &str) -> ConnectionManager {
        let mgr = ConnectionManager::bootstrap();
        let db = AppDatabase::open_in_memory().expect("app db");
        mgr.set_grid_save_store(Arc::new(DbGridSaveStore { db: Arc::new(db) }));
        open(&mgr, path, session_id, "p1", environment).await;
        mgr
    }

    async fn open(
        mgr: &ConnectionManager,
        path: &Path,
        session_id: &str,
        profile: &str,
        environment: &str,
    ) {
        mgr.open_session(OpenSessionRequest {
            engine: "sqlite".into(),
            plugin_config: json!({ "path": path.to_string_lossy() }),
            password: None,
            pool: None,
            session_id: Some(session_id.into()),
            profile_id: Some(profile.into()),
            ssh: None,
            tls_key_passphrase: None,
            environment: Some(environment.into()),
            connection_name: Some(session_id.into()),
            profile_read_only: false,
            use_saved_ssh: true,
        })
        .await
        .expect("open session");
    }

    async fn save_n(mgr: &ConnectionManager, session_id: &str, from: i64, to: i64) -> String {
        let column = |name: &str| TableEditColumn {
            name: name.into(),
            data_type: "integer".into(),
            nullable: true,
            pk: false,
        };
        let result = mgr
            .apply_table_edits(ApplyTableEditsRequest {
                session_id: session_id.into(),
                table: "docs".into(),
                schema: None,
                database: None,
                columns: vec![column("id"), column("n")],
                updates: vec![TableRowUpdate {
                    original: [("id".into(), json!(1)), ("n".into(), json!(from))]
                        .into_iter()
                        .collect(),
                    values: [("n".into(), json!(to))].into_iter().collect(),
                }],
                inserts: Vec::new(),
                deletes: Vec::new(),
                confirm_object_name: None,
                dry_run: false,
            })
            .await
            .expect("save");
        assert!(result.undo.is_none());
        result.save_id.expect("save id")
    }

    fn rollback(session_id: &str, save_id: &str) -> RollbackTableEditsRequest {
        RollbackTableEditsRequest {
            session_id: session_id.into(),
            save_id: save_id.into(),
            confirm_object_name: None,
        }
    }

    #[tokio::test]
    async fn rollback_runs_once() {
        let path = temp_db("once");
        sql(&path, SEED);
        let mgr = manager(&path, "s", "local").await;
        let save_id = save_n(&mgr, "s", 10, 11).await;
        assert_eq!(n_of(&path), 11);
        mgr.rollback_table_edits(rollback("s", &save_id))
            .await
            .expect("rollback");
        assert_eq!(n_of(&path), 10);
        let again = mgr.rollback_table_edits(rollback("s", &save_id)).await;
        assert!(
            matches!(&again, Err(ConnectorError::Policy(msg)) if msg.contains("already rolled back")),
            "{again:?}"
        );
        assert_eq!(n_of(&path), 10);
        let _ = mgr.close_session("s").await;
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn rollback_is_refused_after_the_row_changed() {
        let path = temp_db("stale");
        sql(&path, SEED);
        let mgr = manager(&path, "s", "local").await;
        let save_id = save_n(&mgr, "s", 10, 11).await;
        sql(&path, "UPDATE docs SET n = 12 WHERE id = 1");
        let err = mgr
            .rollback_table_edits(rollback("s", &save_id))
            .await
            .expect_err("stale");
        assert!(err.to_string().contains("rollback_stale"), "{err}");
        assert_eq!(n_of(&path), 12);
        // A stale save is used up: putting the row back does not revive it.
        sql(&path, "UPDATE docs SET n = 11 WHERE id = 1");
        let again = mgr.rollback_table_edits(rollback("s", &save_id)).await;
        assert!(again.is_err());
        assert_eq!(n_of(&path), 11);
        let _ = mgr.close_session("s").await;
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn production_rollback_needs_the_guard_and_keeps_the_save_until_then() {
        let path = temp_db("prod");
        sql(&path, SEED);
        let mgr = manager(&path, "s", "production").await;
        mgr.unlock_writes("s").await.expect("unlock for save");
        let save_id = save_n(&mgr, "s", 10, 11).await;
        mgr.close_session("s").await.expect("close");
        open(&mgr, &path, "s2", "p1", "production").await;
        match mgr.rollback_table_edits(rollback("s2", &save_id)).await {
            Err(ConnectorError::WriteGuard(prompt)) => {
                assert_eq!(prompt.kind, "unlock");
                assert!(
                    prompt.statements.iter().any(|sql| sql.contains("UPDATE")),
                    "{:?}",
                    prompt.statements
                );
            }
            other => panic!("expected the write guard, got {other:?}"),
        }
        assert_eq!(n_of(&path), 11);
        mgr.unlock_writes("s2").await.expect("unlock");
        mgr.rollback_table_edits(rollback("s2", &save_id))
            .await
            .expect("rollback after unlock");
        assert_eq!(n_of(&path), 10);
        let _ = mgr.close_session("s2").await;
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn another_connection_cannot_use_the_save() {
        let path = temp_db("owner");
        sql(&path, SEED);
        let mgr = manager(&path, "s", "local").await;
        let save_id = save_n(&mgr, "s", 10, 11).await;
        open(&mgr, &path, "other", "p2", "local").await;
        let err = mgr
            .rollback_table_edits(rollback("other", &save_id))
            .await
            .expect_err("other connection");
        assert!(err.to_string().contains("another connection"), "{err}");
        assert_eq!(n_of(&path), 11);
        mgr.rollback_table_edits(rollback("s", &save_id))
            .await
            .expect("owner can still roll back");
        let _ = mgr.close_session("s").await;
        let _ = mgr.close_session("other").await;
        let _ = std::fs::remove_file(&path);
    }
}
