//! Grid insert / update / delete. One transaction, one statement per row.

use mystique_connector_api::{
    affected_mismatch, keyless_ambiguous, overlay_catalog_pk, plan_table_edits, rollback_stale,
    rows_hash, ApplyTableEditsInput, ApplyTableEditsResult, ConnectorError, ConnectorResult,
    GridUndo, SqlIdentStyle, SqlPlaceholder,
};
use rusqlite::types::{ToSql, ToSqlOutput, Value as SqlValue};
use serde_json::Value as JsonValue;

use super::catalog;
use super::encode::value_ref_json;
use super::interrupt::map_err;
use super::SqliteSession;

pub(super) async fn apply_table_edits(
    session: &SqliteSession,
    edits: &ApplyTableEditsInput,
) -> ConnectorResult<ApplyTableEditsResult> {
    session.ensure_open()?;
    let described = catalog::describe_table(
        session,
        edits.database.as_deref(),
        edits.schema.as_deref(),
        &edits.table,
    )
    .await?;
    let mut owned = edits.clone();
    overlay_catalog_pk(&mut owned.columns, &described);
    let plan = plan_table_edits(SqlIdentStyle::Postgres, SqlPlaceholder::Question, &owned)?;
    let previews = plan.previews();
    if edits.dry_run {
        return Ok(ApplyTableEditsResult {
            statements: previews,
            ..ApplyTableEditsResult::empty()
        });
    }
    let updated = owned.updates.len() as u32;
    let inserted = owned.inserts.len() as u32;
    let deleted = owned.deletes.len() as u32;
    let pool = session.pool();
    session
        .blocking(move || -> ConnectorResult<ApplyTableEditsResult> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(query_failed_plain)?;
            let run = apply_plan(&conn, &plan).and_then(|()| read_undo(&conn, &plan));
            match run {
                Ok(undo) => {
                    conn.execute_batch("COMMIT").map_err(query_failed_plain)?;
                    Ok(ApplyTableEditsResult {
                        updated,
                        inserted,
                        deleted,
                        statements: previews,
                        save_id: None,
                        undo,
                    })
                }
                Err(err) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    Err(err)
                }
            }
        })
        .await
}

pub(super) async fn apply_grid_undo(
    session: &SqliteSession,
    undo: &GridUndo,
) -> ConnectorResult<()> {
    session.ensure_open()?;
    let undo = undo.clone();
    let pool = session.pool();
    session
        .blocking(move || -> ConnectorResult<()> {
            let conn = pool
                .get()
                .map_err(|e| ConnectorError::Pool(e.to_string()))?;
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(query_failed_plain)?;
            match run_undo(&conn, &undo) {
                Ok(()) => {
                    conn.execute_batch("COMMIT").map_err(query_failed_plain)?;
                    Ok(())
                }
                Err(err) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    Err(err)
                }
            }
        })
        .await
}

/// Post-save rows for the undo. `None` when a row cannot be put back.
fn read_undo(
    conn: &rusqlite::Connection,
    plan: &mystique_connector_api::EditPlan,
) -> ConnectorResult<Option<GridUndo>> {
    if plan.rollback_sql().is_none() {
        return Ok(None);
    }
    let mut rows = Vec::new();
    for edit in &plan.edits {
        match read_rows(conn, &edit.check.sql, &edit.check.params) {
            Ok(found) => rows.push(found),
            Err(_) => return Ok(None),
        }
    }
    Ok(plan.undo(&rows))
}

fn run_undo(conn: &rusqlite::Connection, undo: &GridUndo) -> ConnectorResult<()> {
    for check in &undo.checks {
        let rows = read_rows(conn, &check.sql, &check.params)?;
        if rows_hash(&rows) != check.hash {
            return Err(rollback_stale());
        }
    }
    for sql in &undo.inverse {
        let changed = conn.execute(sql, []).map_err(query_failed_plain)?;
        if changed != 1 {
            return Err(affected_mismatch("rollback", changed as u64));
        }
    }
    Ok(())
}

fn read_rows(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[JsonValue],
) -> ConnectorResult<Vec<Vec<JsonValue>>> {
    let binds = json_binds(params);
    let mut stmt = conn.prepare(sql).map_err(query_failed_plain)?;
    let width = stmt.column_count();
    let mut rows = stmt
        .query(rusqlite::params_from_iter(binds.iter()))
        .map_err(query_failed_plain)?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(query_failed_plain)? {
        let mut cells = Vec::with_capacity(width);
        for index in 0..width {
            let value = row.get_ref(index).map_err(query_failed_plain)?;
            cells.push(value_ref_json(value));
        }
        out.push(cells);
    }
    Ok(out)
}

fn apply_plan(
    conn: &rusqlite::Connection,
    plan: &mystique_connector_api::EditPlan,
) -> ConnectorResult<()> {
    for edit in &plan.edits {
        if let Some(count) = &edit.uniqueness {
            let found = count_rows(conn, &count.sql, &count.params)?;
            if found != 1 {
                return Err(if found > 1 {
                    keyless_ambiguous(found)
                } else {
                    affected_mismatch("write", 0)
                });
            }
        }
        let changed = exec_params(conn, &edit.sql, &edit.params)?;
        if changed != 1 {
            let kind = if edit.sql.starts_with("INSERT") {
                "INSERT"
            } else if edit.sql.starts_with("DELETE") {
                "DELETE"
            } else {
                "UPDATE"
            };
            return Err(affected_mismatch(kind, changed as u64));
        }
    }
    Ok(())
}

fn count_rows(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[JsonValue],
) -> ConnectorResult<i64> {
    let binds = json_binds(params);
    conn.query_row(sql, rusqlite::params_from_iter(binds.iter()), |row| {
        row.get(0)
    })
    .map_err(query_failed_plain)
}

fn exec_params(
    conn: &rusqlite::Connection,
    sql: &str,
    params: &[JsonValue],
) -> ConnectorResult<usize> {
    let binds = json_binds(params);
    conn.execute(sql, rusqlite::params_from_iter(binds.iter()))
        .map_err(query_failed_plain)
}

pub(super) fn json_binds(params: &[JsonValue]) -> Vec<SqlBind> {
    params.iter().cloned().map(SqlBind).collect()
}

pub(super) struct SqlBind(JsonValue);

impl ToSql for SqlBind {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        let value = match &self.0 {
            JsonValue::Null => SqlValue::Null,
            JsonValue::Bool(flag) => SqlValue::Integer(if *flag { 1 } else { 0 }),
            JsonValue::Number(number) => {
                if let Some(integer) = number.as_i64() {
                    SqlValue::Integer(integer)
                } else if let Some(real) = number.as_f64() {
                    SqlValue::Real(real)
                } else {
                    SqlValue::Text(number.to_string())
                }
            }
            JsonValue::String(text) => SqlValue::Text(text.clone()),
            other => SqlValue::Text(other.to_string()),
        };
        Ok(ToSqlOutput::Owned(value))
    }
}

fn query_failed_plain(err: rusqlite::Error) -> ConnectorError {
    map_err(&mystique_connector_api::QueryControl::new(), err)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use mystique_connector_api::{
        ConnectionConfig, DatabaseConnector, PoolConfig, QueryControl, QueryMode, ValidatedQuery,
    };

    use crate::SqliteConnector;
    use serde_json::json;

    use super::*;

    fn temp_path() -> std::path::PathBuf {
        let nanos = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_nanos(),
            Err(_) => 0,
        };
        std::env::temp_dir().join(format!(
            "mystique-save-{}-{nanos}.sqlite",
            std::process::id()
        ))
    }

    async fn session_at(
        path: &std::path::Path,
    ) -> Box<dyn mystique_connector_api::ConnectorSession> {
        match SqliteConnector
            .connect(&ConnectionConfig {
                engine: "sqlite".into(),
                plugin_config: json!({ "path": path.to_string_lossy() }),
                password: None,
                pool: PoolConfig {
                    max_size: 1,
                    min_idle: 0,
                    acquire_timeout_ms: 3_000,
                    idle_timeout_secs: 5,
                },
            })
            .await
        {
            Ok(session) => session,
            Err(err) => panic!("connect: {err}"),
        }
    }

    async fn exec(
        session: &dyn mystique_connector_api::ConnectorSession,
        sql: &str,
        mode: QueryMode,
    ) {
        let query = ValidatedQuery {
            text: sql.into(),
            mode,
            max_rows: 10,
            timeout_ms: 5_000,
            collection: None,
            database: None,
            control: QueryControl::new(),
            params: Vec::new(),
        };
        if let Err(err) = session.execute_query(&query).await {
            panic!("sql: {err}");
        }
    }

    #[tokio::test]
    async fn a_failed_second_row_leaves_the_first_unchanged() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let session = session_at(&path).await;
        exec(
            session.as_ref(),
            "CREATE TABLE docs (id INTEGER PRIMARY KEY, n INTEGER NOT NULL); INSERT INTO docs VALUES (1, 10), (2, 20);",
            QueryMode::Write,
        )
        .await;
        let edits = ApplyTableEditsInput {
            table: "docs".into(),
            schema: None,
            database: None,
            columns: vec![
                mystique_connector_api::TableEditColumn {
                    name: "id".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    pk: false,
                },
                mystique_connector_api::TableEditColumn {
                    name: "n".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    pk: false,
                },
            ],
            updates: vec![
                mystique_connector_api::TableRowUpdate {
                    original: [("id".into(), json!(1)), ("n".into(), json!(10))]
                        .into_iter()
                        .collect(),
                    values: [("n".into(), json!(11))].into_iter().collect(),
                },
                mystique_connector_api::TableRowUpdate {
                    original: [("id".into(), json!(2)), ("n".into(), json!(20))]
                        .into_iter()
                        .collect(),
                    values: [("n".into(), JsonValue::Null)].into_iter().collect(),
                },
            ],
            inserts: Vec::new(),
            deletes: Vec::new(),
            dry_run: false,
        };
        let err = match session.apply_table_edits(&edits).await {
            Ok(_) => panic!("null update should fail"),
            Err(err) => err,
        };
        let message = err.to_string();
        assert!(
            message.contains("cannot be empty") || message.contains("NOT NULL"),
            "{message}"
        );
        let check = ValidatedQuery {
            text: "SELECT n FROM docs WHERE id = 1".into(),
            mode: QueryMode::Read,
            max_rows: 5,
            timeout_ms: 5_000,
            collection: None,
            database: None,
            control: QueryControl::new(),
            params: Vec::new(),
        };
        let rows = match session.execute_query(&check).await {
            Ok(result) => result.rows,
            Err(err) => panic!("read: {err}"),
        };
        let value = rows.first().and_then(|row| row.first());
        assert_eq!(value, Some(&json!(10)));
        let _ = session.close().await;
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn two_updates_commit_together_and_rollback_restores_both() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let session = session_at(&path).await;
        exec(
            session.as_ref(),
            "CREATE TABLE docs (id INTEGER PRIMARY KEY, n INTEGER NOT NULL); INSERT INTO docs VALUES (1, 10), (2, 20);",
            QueryMode::Write,
        )
        .await;
        let edits = ApplyTableEditsInput {
            table: "docs".into(),
            schema: None,
            database: None,
            columns: vec![
                mystique_connector_api::TableEditColumn {
                    name: "id".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    pk: false,
                },
                mystique_connector_api::TableEditColumn {
                    name: "n".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    pk: false,
                },
            ],
            updates: vec![
                mystique_connector_api::TableRowUpdate {
                    original: [("id".into(), json!(1)), ("n".into(), json!(10))]
                        .into_iter()
                        .collect(),
                    values: [("n".into(), json!(11))].into_iter().collect(),
                },
                mystique_connector_api::TableRowUpdate {
                    original: [("id".into(), json!(2)), ("n".into(), json!(20))]
                        .into_iter()
                        .collect(),
                    values: [("n".into(), json!(21))].into_iter().collect(),
                },
            ],
            inserts: Vec::new(),
            deletes: Vec::new(),
            dry_run: false,
        };
        let saved = match session.apply_table_edits(&edits).await {
            Ok(saved) => saved,
            Err(err) => panic!("save: {err}"),
        };
        assert_eq!(saved.updated, 2);
        assert_eq!(saved.statements.len(), 2);
        let undo = saved.undo.clone().expect("undo");
        assert_eq!(undo.inverse.len(), 2);
        assert_eq!(undo.checks.len(), 2);
        let read_n = |sql: &str| {
            let sql = sql.to_string();
            let session = session.as_ref();
            async move {
                let check = ValidatedQuery {
                    text: sql,
                    mode: QueryMode::Read,
                    max_rows: 5,
                    timeout_ms: 5_000,
                    collection: None,
                    database: None,
                    control: QueryControl::new(),
                    params: Vec::new(),
                };
                match session.execute_query(&check).await {
                    Ok(result) => result.rows,
                    Err(err) => panic!("read: {err}"),
                }
            }
        };
        let after = read_n("SELECT n FROM docs ORDER BY id").await;
        assert_eq!(after.first().and_then(|row| row.first()), Some(&json!(11)));
        assert_eq!(after.get(1).and_then(|row| row.first()), Some(&json!(21)));
        if let Err(err) = session.apply_grid_undo(&undo).await {
            panic!("rollback: {err}");
        }
        let restored = read_n("SELECT n FROM docs ORDER BY id").await;
        assert_eq!(
            restored.first().and_then(|row| row.first()),
            Some(&json!(10))
        );
        assert_eq!(
            restored.get(1).and_then(|row| row.first()),
            Some(&json!(20))
        );
        let again = session.apply_grid_undo(&undo).await;
        assert!(
            again
                .as_ref()
                .is_err_and(|err| err.to_string().contains("rollback_stale")),
            "{again:?}"
        );
        let _ = session.close().await;
        let _ = fs::remove_file(&path);
    }

    fn docs_columns() -> Vec<mystique_connector_api::TableEditColumn> {
        ["id", "n"]
            .into_iter()
            .map(|name| mystique_connector_api::TableEditColumn {
                name: name.into(),
                data_type: "integer".into(),
                nullable: name != "id",
                pk: false,
            })
            .collect()
    }

    #[tokio::test]
    async fn rollback_is_refused_once_a_saved_row_changed() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let session = session_at(&path).await;
        exec(
            session.as_ref(),
            "CREATE TABLE docs (id INTEGER PRIMARY KEY, n INTEGER); INSERT INTO docs VALUES (1, 10);",
            QueryMode::Write,
        )
        .await;
        let edits = ApplyTableEditsInput {
            table: "docs".into(),
            schema: None,
            database: None,
            columns: docs_columns(),
            updates: vec![mystique_connector_api::TableRowUpdate {
                original: [("id".into(), json!(1)), ("n".into(), json!(10))]
                    .into_iter()
                    .collect(),
                values: [("n".into(), json!(11))].into_iter().collect(),
            }],
            inserts: Vec::new(),
            deletes: Vec::new(),
            dry_run: false,
        };
        let undo = match session.apply_table_edits(&edits).await {
            Ok(saved) => saved.undo.expect("undo"),
            Err(err) => panic!("save: {err}"),
        };
        exec(
            session.as_ref(),
            "UPDATE docs SET n = 12 WHERE id = 1",
            QueryMode::Write,
        )
        .await;
        let err = match session.apply_grid_undo(&undo).await {
            Ok(()) => panic!("stale rollback ran"),
            Err(err) => err.to_string(),
        };
        assert!(err.contains("rollback_stale"), "{err}");
        let check = ValidatedQuery {
            text: "SELECT n FROM docs WHERE id = 1".into(),
            mode: QueryMode::Read,
            max_rows: 5,
            timeout_ms: 5_000,
            collection: None,
            database: None,
            control: QueryControl::new(),
            params: Vec::new(),
        };
        let rows = match session.execute_query(&check).await {
            Ok(result) => result.rows,
            Err(err) => panic!("read: {err}"),
        };
        assert_eq!(rows.first().and_then(|row| row.first()), Some(&json!(12)));
        let _ = session.close().await;
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn rollback_restores_a_deleted_row_and_removes_an_inserted_one() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let session = session_at(&path).await;
        exec(
            session.as_ref(),
            "CREATE TABLE docs (id INTEGER PRIMARY KEY, n INTEGER); INSERT INTO docs VALUES (1, 10);",
            QueryMode::Write,
        )
        .await;
        let edits = ApplyTableEditsInput {
            table: "docs".into(),
            schema: None,
            database: None,
            columns: docs_columns(),
            updates: Vec::new(),
            inserts: vec![mystique_connector_api::TableRowInsert {
                values: [("id".into(), json!(2)), ("n".into(), json!(20))]
                    .into_iter()
                    .collect(),
            }],
            deletes: vec![mystique_connector_api::TableRowDelete {
                original: [("id".into(), json!(1)), ("n".into(), json!(10))]
                    .into_iter()
                    .collect(),
            }],
            dry_run: false,
        };
        let undo = match session.apply_table_edits(&edits).await {
            Ok(saved) => saved.undo.expect("undo"),
            Err(err) => panic!("save: {err}"),
        };
        if let Err(err) = session.apply_grid_undo(&undo).await {
            panic!("rollback: {err}");
        }
        let check = ValidatedQuery {
            text: "SELECT id, n FROM docs ORDER BY id".into(),
            mode: QueryMode::Read,
            max_rows: 5,
            timeout_ms: 5_000,
            collection: None,
            database: None,
            control: QueryControl::new(),
            params: Vec::new(),
        };
        let rows = match session.execute_query(&check).await {
            Ok(result) => result.rows,
            Err(err) => panic!("read: {err}"),
        };
        assert_eq!(rows, vec![vec![json!(1), json!(10)]]);
        let _ = session.close().await;
        let _ = fs::remove_file(&path);
    }

    #[tokio::test]
    async fn two_identical_keyless_rows_are_refused() {
        let path = temp_path();
        let _ = fs::remove_file(&path);
        let session = session_at(&path).await;
        exec(
            session.as_ref(),
            "CREATE TABLE twins (name TEXT, n INTEGER); INSERT INTO twins VALUES ('a', 1), ('a', 1);",
            QueryMode::Write,
        )
        .await;
        let edits = ApplyTableEditsInput {
            table: "twins".into(),
            schema: None,
            database: None,
            columns: vec![
                mystique_connector_api::TableEditColumn {
                    name: "name".into(),
                    data_type: "text".into(),
                    nullable: true,
                    pk: true,
                },
                mystique_connector_api::TableEditColumn {
                    name: "n".into(),
                    data_type: "integer".into(),
                    nullable: true,
                    pk: false,
                },
            ],
            updates: vec![mystique_connector_api::TableRowUpdate {
                original: [("name".into(), json!("a")), ("n".into(), json!(1))]
                    .into_iter()
                    .collect(),
                values: [("n".into(), json!(2))].into_iter().collect(),
            }],
            inserts: Vec::new(),
            deletes: Vec::new(),
            dry_run: false,
        };
        let err = match session.apply_table_edits(&edits).await {
            Ok(_) => panic!("duplicate keyless rows should be refused"),
            Err(err) => err.to_string(),
        };
        assert!(err.contains("keyless_ambiguous"), "{err}");
        let _ = session.close().await;
        let _ = fs::remove_file(&path);
    }
}
