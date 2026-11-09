//! SQLite end-to-end checks for large values and transfers (no server needed).

use std::path::{Path, PathBuf};

use mystique_connector_api::{
    ExecuteQueryRequest, ExportFormat, ExportObjectRequest, FetchLargeValueRequest,
    LargeCellLocator, QueryMode,
};
use serde_json::json;

use super::{ConnectionManager, OpenSessionRequest};

fn temp(label: &str, ext: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "mystique-ee-{label}-{}-{nanos}.{ext}",
        std::process::id()
    ))
}

async fn open(path: &Path, session_id: &str) -> ConnectionManager {
    let mgr = ConnectionManager::bootstrap();
    mgr.open_session(OpenSessionRequest {
        engine: "sqlite".into(),
        plugin_config: json!({ "path": path.to_string_lossy() }),
        password: None,
        pool: None,
        session_id: Some(session_id.into()),
        profile_id: None,
        ssh: None,
        tls_key_passphrase: None,
        environment: Some("local".into()),
        connection_name: Some(session_id.into()),
        profile_read_only: false,
        use_saved_ssh: false,
    })
    .await
    .expect("open");
    mgr
}

fn req(session_id: &str, text: &str, mode: QueryMode) -> ExecuteQueryRequest {
    ExecuteQueryRequest {
        session_id: session_id.into(),
        text: text.into(),
        mode,
        max_rows: Some(500),
        timeout_ms: Some(30_000),
        confirm_object_name: None,
        collection: None,
        database: None,
        execution_id: None,
        params: Vec::new(),
    }
}

async fn fetch_all(
    mgr: &ConnectionManager,
    sid: &str,
    tab: Option<&str>,
    locator: &LargeCellLocator,
) -> String {
    let mut offset = 0;
    let mut text = String::new();
    loop {
        let page = mgr
            .fetch_large_value(FetchLargeValueRequest {
                session_id: sid.into(),
                locator: locator.clone(),
                offset,
                limit: None,
                execution_id: None,
                tab_id: tab.map(str::to_string),
            })
            .await
            .expect("page");
        text.push_str(&page.text);
        match page.next_offset {
            Some(next) => offset = next,
            None => return text,
        }
    }
}

#[tokio::test]
async fn sqlite_query_cells_get_locators_from_a_parsed_single_table_select() {
    let path = temp("locator", "sqlite");
    let mgr = open(&path, "ee-loc").await;
    let one = format!("{}NEEDLE-ONE", "x".repeat(200_000));
    let two = format!("{}NEEDLE-TWO", "y".repeat(200_000));
    mgr.execute_batch(
        req(
            "ee-loc",
            &format!(
                "CREATE TABLE docs (id INTEGER PRIMARY KEY, body TEXT);\
                 INSERT INTO docs VALUES (9007199254740993, '{one}');\
                 INSERT INTO docs VALUES (9007199254740995, '{two}');"
            ),
            QueryMode::Write,
        ),
        None,
    )
    .await
    .expect("seed");

    let sql = "SELECT id, body AS content FROM docs ORDER BY id";
    let mut batch = mgr
        .execute_batch(req("ee-loc", sql, QueryMode::Read), None)
        .await
        .expect("read");
    mgr.clip_batch_for("ee-loc", sql, &mut batch).await;
    let rows = &batch.results[0].result.as_ref().expect("rows").rows;
    let encoded = serde_json::to_string(rows).expect("json");
    assert!(!encoded.contains("NEEDLE"), "the payload crossed IPC");
    let locator: LargeCellLocator =
        serde_json::from_value(rows[0][1]["locator"].clone()).expect("locator");
    assert_eq!(locator.table, "docs");
    assert_eq!(locator.column, "body");
    let text = fetch_all(&mgr, "ee-loc", None, &locator).await;
    assert!(text.ends_with("NEEDLE-ONE"), "loaded the wrong row");
    assert!(!text.contains("NEEDLE-TWO"));

    // An expression or a missing key: the preview says so, no locator.
    let sql = "SELECT body || '' FROM docs";
    let mut batch = mgr
        .execute_batch(req("ee-loc", sql, QueryMode::Read), None)
        .await
        .expect("read");
    mgr.clip_batch_for("ee-loc", sql, &mut batch).await;
    let rows = &batch.results[0].result.as_ref().expect("rows").rows;
    assert!(rows[0][0].get("$large").is_some());
    assert!(rows[0][0].get("locator").is_none());

    let _ = mgr.close_session("ee-loc").await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn sqlite_export_reads_through_the_open_transaction_and_streams() {
    let path = temp("export", "sqlite");
    let mgr = open(&path, "ee-exp").await;
    mgr.execute_batch(
        req(
            "ee-exp",
            "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT);\
             WITH RECURSIVE g(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM g WHERE n < 5000) \
             INSERT INTO t SELECT n, 'row ' || n FROM g;",
            QueryMode::Write,
        ),
        None,
    )
    .await
    .expect("seed");
    mgr.pin_tab("ee-exp", "tab-1").await.expect("pin");
    mgr.execute_pinned_batch(
        req(
            "ee-exp",
            "INSERT INTO t VALUES (9999, 'uncommitted')",
            QueryMode::Write,
        ),
        "tab-1".into(),
        None,
    )
    .await
    .expect("insert");

    let export = |out: &Path, tab: Option<&str>| ExportObjectRequest {
        session_id: "ee-exp".into(),
        name: "t".into(),
        schema: None,
        database: None,
        kind: None,
        format: ExportFormat::Csv,
        path: out.to_string_lossy().into_owned(),
        execution_id: None,
        tab_id: tab.map(str::to_string),
    };
    let through = temp("through", "csv");
    let done = mgr
        .export_object(export(&through, Some("tab-1")), None)
        .await
        .expect("export through tab");
    assert_eq!(done.row_count, 5001);
    let text = std::fs::read_to_string(&through).expect("read");
    assert!(text.contains("uncommitted"));
    assert!(text.starts_with("id,name\n1,row 1\n"), "key order");

    mgr.rollback_tab("tab-1").await.expect("rollback");
    let after = temp("after", "csv");
    let done = mgr
        .export_object(export(&after, Some("tab-1")), None)
        .await
        .expect("export after rollback");
    assert_eq!(done.row_count, 5000, "nothing pinned: the pool reads");
    assert_eq!(mgr.running_transfers(), 0);

    let _ = mgr.close_session("ee-exp").await;
    for file in [path, through, after] {
        let _ = std::fs::remove_file(file);
    }
}
