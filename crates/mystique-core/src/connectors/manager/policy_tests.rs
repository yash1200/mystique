//! The timed write unlock (F-04) and the row preview (F-05).

use std::time::Duration;

use mystique_connector_api::{ConnectorError, ExecuteQueryRequest, QueryMode};
use serde_json::json;

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

#[tokio::test]
async fn row_preview_reads_the_rows_and_never_writes() {
    let path = temp_db("row-preview");
    let mgr = seeded(&path).await;
    mgr.open_session(open_req(&path, "prod", "production", false))
        .await
        .expect("open");
    mgr.unlock_writes("prod").await.expect("unlock");
    mgr.execute_query(write("prod", "INSERT INTO t VALUES (1), (2), (3)"))
        .await
        .expect("seed rows");

    let preview = |statement: &str| {
        mgr.preview_write_rows(super::PreviewWriteRowsRequest {
            session_id: "prod".into(),
            statement: statement.into(),
            database: None,
        })
    };
    let update = preview("UPDATE t SET id = id * 10 WHERE id >= 2")
        .await
        .expect("preview");
    assert!(update.available, "{update:?}");
    assert_eq!(update.total, Some(2));
    assert_eq!(update.new_columns, ["id (new)"]);
    assert_eq!(update.rows.len(), 2);
    assert_eq!(
        update.columns.last().map(|c| c.name.as_str()),
        Some("id (new)")
    );

    let delete = preview("DELETE FROM t").await.expect("preview");
    assert_eq!(delete.total, Some(3));

    let insert = preview("INSERT INTO t VALUES (7), (8)")
        .await
        .expect("preview");
    assert_eq!(insert.total, Some(2));
    assert_eq!(insert.columns[0].name, "id");

    let multi = preview("UPDATE t SET id = 1; DELETE FROM t")
        .await
        .expect("preview");
    assert!(!multi.available);

    let count = mgr
        .execute_query(write("prod", "SELECT COUNT(*), SUM(id) FROM t"))
        .await
        .expect("count");
    assert_eq!(count.rows[0][0], json!(3), "the preview wrote");
    assert_eq!(count.rows[0][1], json!(6), "the preview wrote");
    let _ = mgr.close_session("prod").await;
    let _ = std::fs::remove_file(&path);
}
