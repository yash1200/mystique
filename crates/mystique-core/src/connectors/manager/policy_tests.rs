//! Per-database production (F-03) and the timed write unlock (F-04).

use std::time::Duration;

use mystique_connector_api::{
    sql_write_targets, ConnectorError, DatabaseEnvironment, ExecuteQueryRequest, QueryMode,
    SqlDialect, WriteAction, WriteRisk, WriteTargets,
};
use serde_json::json;

use super::guard::WriteCheck;
use super::{ConnectionManager, OpenSessionRequest, SessionOrigin};

fn temp_db(label: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "mystique-{label}-{}-{nanos}.sqlite",
        std::process::id()
    ))
}

fn open_req(
    path: &std::path::Path,
    session_id: &str,
    environment: &str,
    read_only: bool,
) -> OpenSessionRequest {
    OpenSessionRequest {
        engine: "sqlite".into(),
        plugin_config: json!({ "path": path.to_string_lossy() }),
        password: None,
        pool: None,
        session_id: Some(session_id.into()),
        profile_id: Some("7".into()),
        ssh: None,
        tls_key_passphrase: None,
        environment: Some(environment.into()),
        connection_name: Some("Dev server".into()),
        profile_read_only: read_only,
        use_saved_ssh: true,
    }
}

fn write(session_id: &str, text: &str) -> ExecuteQueryRequest {
    ExecuteQueryRequest {
        session_id: session_id.into(),
        text: text.into(),
        mode: QueryMode::Write,
        max_rows: None,
        timeout_ms: None,
        confirm_object_name: None,
        collection: None,
        database: None,
        execution_id: None,
        params: Vec::new(),
    }
}

fn mark(database: &str, environment: &str) -> DatabaseEnvironment {
    DatabaseEnvironment {
        database: database.into(),
        schema: None,
        environment: environment.into(),
    }
}

async fn seeded(path: &std::path::Path) -> ConnectionManager {
    let _ = std::fs::remove_file(path);
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(open_req(path, "seed", "local", false))
        .await
        .expect("open seed");
    mgr.execute_query(write("seed", "CREATE TABLE t (id INTEGER)"))
        .await
        .expect("seed");
    mgr.close_session("seed").await.expect("close seed");
    mgr
}

/// The environment the guard asks about, or `None` when the write runs.
async fn asks(mgr: &ConnectionManager, session_id: &str, targets: WriteTargets) -> Option<String> {
    mgr.write_prompt(WriteCheck {
        statements: vec!["UPDATE t SET a = 1".into()],
        targets: Some(targets),
        ..WriteCheck::new(session_id, WriteAction::Query, WriteRisk::Write)
    })
    .await
    .expect("policy")
    .map(|prompt| {
        format!(
            "{} {}",
            prompt.environment,
            prompt.database.unwrap_or_default()
        )
        .trim()
        .to_string()
    })
}

fn mysql(sql: &str, current: &str) -> WriteTargets {
    sql_write_targets(sql, SqlDialect::Mysql, Some(current))
}

#[tokio::test]
async fn a_marked_database_on_a_local_connection_prompts_only_for_writes_there() {
    let path = temp_db("per-db");
    let mgr = seeded(&path).await;
    mgr.open_session(open_req(&path, "dev", "local", false))
        .await
        .expect("open");
    mgr.set_database_marks("7", vec![mark("prod", "production")]);

    assert_eq!(
        asks(&mgr, "dev", mysql("UPDATE prod.t SET a = 1", "scratch")).await,
        Some("production prod".into())
    );
    assert_eq!(
        asks(&mgr, "dev", mysql("UPDATE scratch.t SET a = 1", "scratch")).await,
        None
    );
    assert_eq!(
        asks(&mgr, "dev", mysql("UPDATE t SET a = 1", "scratch")).await,
        None
    );
    // `USE prod` earlier in the batch moves the unqualified write.
    assert_eq!(
        asks(
            &mgr,
            "dev",
            mysql("USE prod; UPDATE t SET a = 1", "scratch")
        )
        .await,
        Some("production prod".into())
    );
    // A write the parser cannot place counts as production.
    assert_eq!(
        asks(&mgr, "dev", mysql("CALL refresh_all()", "scratch")).await,
        Some("production".into())
    );
    // The most dangerous target wins.
    assert_eq!(
        asks(
            &mgr,
            "dev",
            mysql(
                "UPDATE scratch.t SET a = 1; DELETE FROM prod.t WHERE id = 1",
                "scratch"
            )
        )
        .await,
        Some("production prod".into())
    );

    mgr.set_database_marks("7", Vec::new());
    assert_eq!(
        asks(&mgr, "dev", mysql("UPDATE prod.t SET a = 1", "scratch")).await,
        None,
        "no marks: the connection environment again"
    );
    let _ = mgr.close_session("dev").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn a_local_mark_relaxes_one_database_of_a_production_connection() {
    let path = temp_db("per-db-relax");
    let mgr = seeded(&path).await;
    mgr.open_session(open_req(&path, "prod", "production", false))
        .await
        .expect("open");
    mgr.set_database_marks("7", vec![mark("scratch", "local")]);
    assert_eq!(
        asks(&mgr, "prod", mysql("UPDATE scratch.t SET a = 1", "app")).await,
        None
    );
    assert_eq!(
        asks(&mgr, "prod", mysql("UPDATE t SET a = 1", "app")).await,
        Some("production".into())
    );
    assert_eq!(
        asks(&mgr, "prod", WriteTargets::unresolved()).await,
        Some("production".into())
    );
    let _ = mgr.close_session("prod").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn the_query_path_reads_targets_off_the_statement() {
    // SQLite is one database (`main`); marking it production prompts.
    let path = temp_db("per-db-query");
    let mgr = seeded(&path).await;
    mgr.open_session(open_req(&path, "dev", "local", false))
        .await
        .expect("open");
    mgr.execute_query(write("dev", "INSERT INTO t VALUES (1)"))
        .await
        .expect("local write runs");
    mgr.set_database_marks("7", vec![mark("main", "production")]);
    let err = mgr
        .execute_query(write("dev", "INSERT INTO t VALUES (2)"))
        .await
        .expect_err("marked database asks");
    let ConnectorError::WriteGuard(prompt) = err else {
        panic!("expected a prompt, got {err:?}");
    };
    assert_eq!(prompt.environment, "production");
    assert_eq!(prompt.database.as_deref(), Some("main"));
    assert_eq!(prompt.kind, "unlock");
    let _ = mgr.close_session("dev").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn timed_unlock_allows_writes_until_it_expires() {
    let path = temp_db("timed-unlock");
    let mgr = seeded(&path).await;
    mgr.open_session(open_req(&path, "ro", "local", true))
        .await
        .expect("open read-only");
    mgr.execute_query(write("ro", "INSERT INTO t VALUES (1)"))
        .await
        .expect_err("read-only refuses the write");

    let status = mgr
        .start_timed_unlock_for("ro", Duration::from_millis(700))
        .await
        .expect("unlock");
    assert!(status.remaining_ms.is_some_and(|ms| ms <= 700));
    mgr.execute_query(write("ro", "INSERT INTO t VALUES (1)"))
        .await
        .expect("write during the unlock");

    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(mgr.timed_unlock_status("ro").remaining_ms, None);
    mgr.execute_query(write("ro", "INSERT INTO t VALUES (2)"))
        .await
        .expect_err("the next write after expiry is refused");

    // The profile bit never changed.
    let (_, read_only) = mgr.session_and_read_only("ro").await.expect("session");
    assert!(read_only);
    let _ = mgr.close_session("ro").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn timed_unlock_offers_one_or_five_minutes_and_ends_early() {
    let path = temp_db("timed-choices");
    let mgr = seeded(&path).await;
    mgr.open_session(open_req(&path, "ro", "local", true))
        .await
        .expect("open read-only");
    assert!(mgr.start_timed_unlock("ro", 30).await.is_err());
    assert!(mgr.start_timed_unlock("ro", 3_600).await.is_err());
    let status = mgr.start_timed_unlock("ro", 300).await.expect("five");
    let left = status.remaining_ms.expect("running");
    assert!(left > 290_000 && left <= 300_000, "{left}");
    assert_eq!(mgr.end_timed_unlock("ro").remaining_ms, None);

    mgr.start_timed_unlock("ro", 60).await.expect("one");
    mgr.close_session("ro").await.expect("close");
    mgr.open_session(open_req(&path, "ro", "local", true))
        .await
        .expect("reopen");
    assert_eq!(
        mgr.timed_unlock_status("ro").remaining_ms,
        None,
        "a disconnect ends the unlock"
    );

    // A connection that is not read-only has nothing to unlock.
    mgr.open_session(open_req(&path, "rw", "local", false))
        .await
        .expect("open");
    assert!(mgr.start_timed_unlock("rw", 60).await.is_err());
    let _ = mgr.close_session("ro").await;
    let _ = mgr.close_session("rw").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn agent_and_mcp_sessions_never_get_the_human_unlock() {
    let path = temp_db("timed-agent");
    let mgr = seeded(&path).await;
    mgr.open_session(open_req(&path, "7", "local", true))
        .await
        .expect("open desktop");
    mgr.open_session_as(open_req(&path, "mcp:7", "local", true), SessionOrigin::Mcp)
        .await
        .expect("open mcp");
    mgr.start_timed_unlock("7", 60).await.expect("human unlock");
    mgr.execute_query(write("7", "INSERT INTO t VALUES (1)"))
        .await
        .expect("the person may write");
    mgr.execute_query(write("mcp:7", "INSERT INTO t VALUES (2)"))
        .await
        .expect_err("MCP stays read-only during the human's unlock");
    assert!(mgr.start_timed_unlock("mcp:7", 60).await.is_err());
    let _ = mgr.close_session("7").await;
    let _ = mgr.close_session("mcp:7").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn production_still_asks_during_a_timed_unlock() {
    let path = temp_db("timed-prod");
    let mgr = seeded(&path).await;
    mgr.open_session(open_req(&path, "ro", "production", true))
        .await
        .expect("open");
    mgr.start_timed_unlock("ro", 60).await.expect("unlock");
    let err = mgr
        .execute_query(write("ro", "INSERT INTO t VALUES (1)"))
        .await
        .expect_err("production asks on top of the timer");
    assert!(matches!(err, ConnectorError::WriteGuard(_)), "{err:?}");
    let _ = mgr.close_session("ro").await;
    let _ = std::fs::remove_file(&path);
}
