//! Live checks for large values (E-07) and transfers (E-11) against local
//! Postgres and MySQL.
//!
//! `cargo test -p mystique-core --test live_transfer -- --ignored --nocapture`
//!
//! Tables are prefixed `mq_ee_` and dropped at the end of each test.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use mystique_connector_api::{
    DescribeTableRequest, ImportEncoding, ImportFormat, ImportOnDuplicate, LargeCellLocator,
};
use mystique_core::{
    ConnectionManager, ExecuteQueryRequest, ExportFormat, ExportObjectRequest,
    FetchLargeValueRequest, ImportObjectRequest, ListRelationshipsRequest, OpenSessionRequest,
    QueryBatchResult, QueryMode, RowProgress,
};
use serde_json::{json, Value as JsonValue};

fn user() -> String {
    std::env::var("USER").unwrap_or_else(|_| "postgres".into())
}

fn open(engine: &str, session_id: &str) -> OpenSessionRequest {
    let plugin_config = match engine {
        "postgres" => json!({
            "host": "127.0.0.1",
            "port": 5432,
            "database": "postgres",
            "username": user()
        }),
        _ => json!({
            "host": "127.0.0.1",
            "port": 3306,
            "database": "mystique",
            "username": "admin"
        }),
    };
    OpenSessionRequest {
        engine: engine.into(),
        plugin_config,
        password: (engine == "mysql").then(|| "123".to_string()),
        pool: None,
        session_id: Some(session_id.into()),
        profile_id: None,
        ssh: None,
        tls_key_passphrase: None,
        environment: Some("local".into()),
        connection_name: Some(session_id.into()),
        profile_read_only: false,
        use_saved_ssh: false,
    }
}

fn req(session_id: &str, text: &str, mode: QueryMode) -> ExecuteQueryRequest {
    ExecuteQueryRequest {
        session_id: session_id.into(),
        text: text.into(),
        mode,
        max_rows: Some(1_000),
        timeout_ms: Some(120_000),
        confirm_object_name: Some("mq_ee".into()),
        collection: None,
        database: None,
        execution_id: None,
        params: Vec::new(),
    }
}

async fn write(mgr: &ConnectionManager, session_id: &str, sql: &str) {
    let batch = mgr
        .execute_batch(req(session_id, sql, QueryMode::Write), None)
        .await
        .unwrap_or_else(|err| panic!("{sql}: {err}"));
    for stmt in &batch.results {
        assert!(stmt.error.is_none(), "{sql}: {:?}", stmt.error);
    }
}

/// Run a read and clip it the way the grid IPC does.
async fn grid_read(mgr: &ConnectionManager, session_id: &str, sql: &str) -> QueryBatchResult {
    let mut batch = mgr
        .execute_batch(req(session_id, sql, QueryMode::Read), None)
        .await
        .unwrap_or_else(|err| panic!("{sql}: {err}"));
    mgr.clip_batch_for(session_id, sql, &mut batch).await;
    batch
}

fn locator_of(cell: &JsonValue) -> Option<LargeCellLocator> {
    serde_json::from_value(cell.get("locator")?.clone()).ok()
}

async fn load_all(
    mgr: &ConnectionManager,
    session_id: &str,
    tab_id: Option<&str>,
    locator: &LargeCellLocator,
) -> String {
    let mut offset = 0;
    let mut text = String::new();
    loop {
        let page = mgr
            .fetch_large_value(FetchLargeValueRequest {
                session_id: session_id.into(),
                locator: locator.clone(),
                offset,
                limit: None,
                execution_id: None,
                tab_id: tab_id.map(str::to_string),
            })
            .await
            .unwrap_or_else(|err| panic!("fetch at {offset}: {err}"));
        text.push_str(&page.text);
        match page.next_offset {
            Some(next) => offset = next,
            None => return text,
        }
    }
}

fn temp_file(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("mq_ee_{}_{name}", std::process::id()))
}

fn rss_kb() -> u64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output();
    out.ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn export_req(
    session_id: &str,
    table: &str,
    schema: Option<&str>,
    path: &std::path::Path,
    format: ExportFormat,
    tab_id: Option<&str>,
) -> ExportObjectRequest {
    ExportObjectRequest {
        session_id: session_id.into(),
        name: table.into(),
        schema: schema.map(str::to_string),
        database: None,
        kind: None,
        format,
        path: path.to_string_lossy().into_owned(),
        execution_id: None,
        tab_id: tab_id.map(str::to_string),
    }
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn pg_query_cells_carry_locators_and_load_by_a_typed_key() {
    let mgr = ConnectionManager::bootstrap();
    let sid = "mq_ee_pg_locators";
    mgr.open_session(open("postgres", sid)).await.expect("open");
    write(
        &mgr,
        sid,
        "DROP TABLE IF EXISTS mq_ee_docs; \
         CREATE TABLE mq_ee_docs (id int8 PRIMARY KEY, title text, body text); \
         INSERT INTO mq_ee_docs SELECT g, 't' || g, repeat('x', 50000) || 'ROW-' || g \
           FROM generate_series(9007199254740990, 9007199254740995) g",
    )
    .await;

    // Aliased key and value columns; the locator names the table columns.
    let batch = grid_read(
        &mgr,
        sid,
        "SELECT d.id AS doc_id, d.body AS content, length(d.body) AS n \
         FROM mq_ee_docs d WHERE d.id = 9007199254740993",
    )
    .await;
    let rows = &batch.results[0].result.as_ref().expect("rows").rows;
    let locator = locator_of(&rows[0][1]).expect("locator on the clipped cell");
    assert_eq!(locator.table, "mq_ee_docs");
    assert_eq!(locator.schema.as_deref(), Some("public"));
    assert_eq!(locator.column, "body");
    assert_eq!(locator.keys[0].column, "id");
    assert_eq!(locator.keys[0].value, json!("9007199254740993"));
    let text = load_all(&mgr, sid, None, &locator).await;
    assert!(text.ends_with("ROW-9007199254740993"), "wrong row");
    assert_eq!(text.chars().count(), 50_000 + "ROW-9007199254740993".len());

    // A join keeps the locator for the table whose key is present.
    let batch = grid_read(
        &mgr,
        sid,
        "SELECT a.id, a.body, b.title FROM mq_ee_docs a \
         JOIN mq_ee_docs b ON b.id = a.id WHERE a.id = 9007199254740991",
    )
    .await;
    let rows = &batch.results[0].result.as_ref().expect("rows").rows;
    assert!(locator_of(&rows[0][1]).is_some(), "join with key present");

    // Without the key column, or as an expression: preview only.
    let batch = grid_read(
        &mgr,
        sid,
        "SELECT body, body || '' AS copy FROM mq_ee_docs LIMIT 1",
    )
    .await;
    let rows = &batch.results[0].result.as_ref().expect("rows").rows;
    assert!(rows[0][0].get("$large").is_some());
    assert!(locator_of(&rows[0][0]).is_none(), "no key in the result");
    assert!(locator_of(&rows[0][1]).is_none(), "expression column");

    // The key match uses the primary key index (no cast on the column).
    let plan = mgr
        .execute_batch(
            ExecuteQueryRequest {
                params: vec![json!("9007199254740993")],
                ..req(
                    sid,
                    "EXPLAIN SELECT \"body\" FROM \"public\".\"mq_ee_docs\" WHERE \"id\" = $1 LIMIT 1",
                    QueryMode::Read,
                )
            },
            None,
        )
        .await
        .expect("explain");
    let plan_text =
        serde_json::to_string(&plan.results[0].result.as_ref().expect("plan").rows).expect("json");
    assert!(plan_text.contains("Index"), "{plan_text}");

    // A row changed under a kept value: offset 0 reads it again.
    write(
        &mgr,
        sid,
        "UPDATE mq_ee_docs SET body = repeat('y', 40000) || 'NEW' WHERE id = 9007199254740993",
    )
    .await;
    let text = load_all(&mgr, sid, None, &locator).await;
    assert!(text.ends_with("NEW"));

    write(&mgr, sid, "DROP TABLE mq_ee_docs").await;
    let _ = mgr.close_session(sid).await;
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn pg_export_and_large_values_read_through_the_tab_transaction() {
    let mgr = ConnectionManager::bootstrap();
    let sid = "mq_ee_pg_txn";
    let tab = "mq_ee_tab_pg";
    mgr.open_session(open("postgres", sid)).await.expect("open");
    write(
        &mgr,
        sid,
        "DROP TABLE IF EXISTS mq_ee_txn; \
         CREATE TABLE mq_ee_txn (id int4 PRIMARY KEY, body text); \
         INSERT INTO mq_ee_txn VALUES (1, 'committed')",
    )
    .await;
    mgr.pin_tab(sid, tab).await.expect("pin");
    let ran = mgr
        .execute_pinned_batch(
            req(
                sid,
                "INSERT INTO mq_ee_txn VALUES (2, repeat('u', 40000) || 'UNCOMMITTED')",
                QueryMode::Write,
            ),
            tab.into(),
            None,
        )
        .await
        .expect("insert in txn");
    assert!(ran.results[0].error.is_none());
    assert!(mgr.pin_status(tab).await.in_transaction);

    let through = temp_file("pg_txn.jsonl");
    let done = mgr
        .export_object(
            export_req(
                sid,
                "mq_ee_txn",
                Some("public"),
                &through,
                ExportFormat::Jsonl,
                Some(tab),
            ),
            None,
        )
        .await
        .expect("export through tab");
    assert_eq!(done.row_count, 2);
    let text = std::fs::read_to_string(&through).expect("read");
    assert!(text.contains("UNCOMMITTED"));

    let outside = temp_file("pg_pool.jsonl");
    let done = mgr
        .export_object(
            export_req(
                sid,
                "mq_ee_txn",
                Some("public"),
                &outside,
                ExportFormat::Jsonl,
                None,
            ),
            None,
        )
        .await
        .expect("export from pool");
    assert_eq!(
        done.row_count, 1,
        "the pool does not see the open transaction"
    );

    // The uncommitted large value loads through the tab, not the pool.
    let mut batch = mgr
        .execute_pinned_batch(
            req(
                sid,
                "SELECT id, body FROM mq_ee_txn WHERE id = 2",
                QueryMode::Read,
            ),
            tab.into(),
            None,
        )
        .await
        .expect("read in txn");
    mgr.clip_batch_for(
        sid,
        "SELECT id, body FROM mq_ee_txn WHERE id = 2",
        &mut batch,
    )
    .await;
    let rows = &batch.results[0].result.as_ref().expect("rows").rows;
    let locator = locator_of(&rows[0][1]).expect("locator");
    let text = load_all(&mgr, sid, Some(tab), &locator).await;
    assert!(text.ends_with("UNCOMMITTED"));
    let pooled = mgr
        .fetch_large_value(FetchLargeValueRequest {
            session_id: sid.into(),
            locator: locator.clone(),
            offset: 0,
            limit: None,
            execution_id: None,
            tab_id: None,
        })
        .await;
    assert!(pooled.is_err(), "the pool cannot see the row");

    // A failing read inside the transaction leaves it usable.
    let bad = LargeCellLocator {
        column: "no_such_column".into(),
        ..locator
    };
    let failed = mgr
        .fetch_large_value(FetchLargeValueRequest {
            session_id: sid.into(),
            locator: bad,
            offset: 0,
            limit: None,
            execution_id: None,
            tab_id: Some(tab.into()),
        })
        .await;
    assert!(failed.is_err());
    let still = mgr
        .execute_pinned_batch(
            req(sid, "SELECT count(*) FROM mq_ee_txn", QueryMode::Read),
            tab.into(),
            None,
        )
        .await
        .expect("txn still usable");
    assert!(
        still.results[0].error.is_none(),
        "{:?}",
        still.results[0].error
    );
    assert_eq!(
        still.results[0].result.as_ref().expect("count").rows[0][0],
        json!(2)
    );

    mgr.rollback_tab(tab).await.expect("rollback");
    write(&mgr, sid, "DROP TABLE mq_ee_txn").await;
    let _ = std::fs::remove_file(&through);
    let _ = std::fs::remove_file(&outside);
    let _ = mgr.close_session(sid).await;
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn pg_streaming_export_of_200k_rows_stays_bounded() {
    let mgr = ConnectionManager::bootstrap();
    let sid = "mq_ee_pg_big";
    mgr.open_session(open("postgres", sid)).await.expect("open");
    write(
        &mgr,
        sid,
        "DROP TABLE IF EXISTS mq_ee_big; \
         CREATE TABLE mq_ee_big (id int8 PRIMARY KEY, name text, payload text, at timestamptz); \
         INSERT INTO mq_ee_big SELECT g, 'name ' || g, repeat(md5(g::text), 6), now() \
           FROM generate_series(1, 200000) g",
    )
    .await;
    for format in [ExportFormat::Csv, ExportFormat::Json] {
        let path = temp_file(&format!("pg_big.{}", format.as_str()));
        let before = rss_kb();
        let peak = Arc::new(AtomicU64::new(before));
        let calls = Arc::new(AtomicU64::new(0));
        let progress = {
            let peak = Arc::clone(&peak);
            let calls = Arc::clone(&calls);
            RowProgress(Arc::new(move |_rows| {
                calls.fetch_add(1, Ordering::SeqCst);
                peak.fetch_max(rss_kb(), Ordering::SeqCst);
            }))
        };
        let started = Instant::now();
        let done = mgr
            .export_object(
                export_req(sid, "mq_ee_big", Some("public"), &path, format, None),
                Some(progress),
            )
            .await
            .expect("export");
        let elapsed = started.elapsed();
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let peak = peak.load(Ordering::SeqCst);
        eprintln!(
            "pg export {}: {} rows, {} MB file, {:.2}s ({:.0} rows/s), rss {} MB -> peak {} MB, {} progress events",
            format.as_str(),
            done.row_count,
            size / (1024 * 1024),
            elapsed.as_secs_f64(),
            done.row_count as f64 / elapsed.as_secs_f64(),
            before / 1024,
            peak / 1024,
            calls.load(Ordering::SeqCst),
        );
        assert_eq!(done.row_count, 200_000);
        // The file is tens of MB; the process must not grow by the file size.
        assert!(
            peak.saturating_sub(before) < size / 1024 / 2,
            "memory grew {} KB for a {} KB file",
            peak.saturating_sub(before),
            size / 1024
        );
        if format == ExportFormat::Csv {
            let text = std::fs::read_to_string(&path).expect("read");
            assert_eq!(text.lines().count(), 200_001);
            assert!(text.lines().nth(1).is_some_and(|l| l.starts_with("1,")));
        }
        let _ = std::fs::remove_file(&path);
    }

    // Cancel mid-export: the destination is never written.
    let path = temp_file("pg_cancel.csv");
    let exec = "mq_ee_cancel_export".to_string();
    let cancel_mgr = &mgr;
    let (done, _) = tokio::join!(
        cancel_mgr.export_object(
            ExportObjectRequest {
                execution_id: Some(exec.clone()),
                ..export_req(
                    sid,
                    "mq_ee_big",
                    Some("public"),
                    &path,
                    ExportFormat::Csv,
                    None
                )
            },
            None,
        ),
        async {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            cancel_mgr.cancel_query(&exec).await.expect("cancel");
        }
    );
    assert!(done.is_err(), "cancelled export reported success");
    assert!(!path.exists(), "a cancelled export left a file");
    assert_eq!(mgr.running_transfers(), 0);

    write(&mgr, sid, "DROP TABLE mq_ee_big").await;
    let _ = mgr.close_session(sid).await;
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn pg_import_writes_in_batches_with_progress_and_stops_on_cancel() {
    let mgr = ConnectionManager::bootstrap();
    let sid = "mq_ee_pg_import";
    mgr.open_session(open("postgres", sid)).await.expect("open");
    write(
        &mgr,
        sid,
        "DROP TABLE IF EXISTS mq_ee_imp; CREATE TABLE mq_ee_imp (id int8 PRIMARY KEY, name text)",
    )
    .await;
    let path = temp_file("pg_import.csv");
    let mut csv = String::from("id,name\n");
    for i in 1..=20_000 {
        csv.push_str(&format!("{i},row {i}\n"));
    }
    std::fs::write(&path, csv).expect("csv");
    let import = |execution_id: Option<String>| ImportObjectRequest {
        session_id: sid.into(),
        name: "mq_ee_imp".into(),
        schema: Some("public".into()),
        database: None,
        kind: None,
        path: path.to_string_lossy().into_owned(),
        format: ImportFormat::Csv,
        encoding: ImportEncoding::Utf8,
        mapping: Vec::new(),
        on_duplicate: ImportOnDuplicate::UpdateOnDuplicate,
        execution_id,
    };
    let reports = Arc::new(std::sync::Mutex::new(Vec::new()));
    let progress = {
        let reports = Arc::clone(&reports);
        RowProgress(Arc::new(move |rows| {
            reports.lock().expect("lock").push(rows);
        }))
    };
    let started = Instant::now();
    let done = mgr
        .import_object(import(None), Some(progress))
        .await
        .expect("import");
    eprintln!(
        "pg import: {} rows in {:.2}s",
        done.inserted + done.updated,
        started.elapsed().as_secs_f64()
    );
    assert_eq!(done.inserted + done.updated, 20_000);
    let reports = reports.lock().expect("lock").clone();
    assert!(reports.len() >= 20_000 / 500, "{reports:?}");
    assert_eq!(reports.last().copied(), Some(20_000));

    // Cancel after the first batch: earlier batches stay, and say so.
    write(&mgr, sid, "TRUNCATE mq_ee_imp").await;
    let exec = "mq_ee_cancel_import".to_string();
    let (outcome, _) = tokio::join!(mgr.import_object(import(Some(exec.clone())), None), async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        mgr.cancel_query(&exec).await.expect("cancel");
    });
    match outcome {
        Err(err) => {
            let message = err.to_string();
            assert!(message.contains("cancelled"), "{message}");
        }
        Ok(done) => panic!("import finished before the cancel: {done:?}"),
    }

    write(&mgr, sid, "DROP TABLE mq_ee_imp").await;
    let _ = std::fs::remove_file(&path);
    let _ = mgr.close_session(sid).await;
}

#[tokio::test]
#[ignore = "needs local Postgres"]
async fn pg_relationships_pair_composite_keys_and_the_cache_follows_ddl() {
    let mgr = ConnectionManager::bootstrap();
    let sid = "mq_ee_pg_rel";
    mgr.open_session(open("postgres", sid)).await.expect("open");
    write(
        &mgr,
        sid,
        "DROP TABLE IF EXISTS mq_ee_child_a, mq_ee_child_b, mq_ee_parent; \
         CREATE TABLE mq_ee_parent (a int, b int, PRIMARY KEY (a, b)); \
         CREATE TABLE mq_ee_child_a (x int, y int, \
           CONSTRAINT mq_ee_fk FOREIGN KEY (x, y) REFERENCES mq_ee_parent (a, b)); \
         CREATE TABLE mq_ee_child_b (p int, q int, \
           CONSTRAINT mq_ee_fk FOREIGN KEY (q, p) REFERENCES mq_ee_parent (a, b))",
    )
    .await;
    let rels = mgr
        .list_relationships(ListRelationshipsRequest {
            session_id: sid.into(),
            database: None,
        })
        .await
        .expect("relationships");
    let mut ours: Vec<(String, String, String)> = rels
        .iter()
        .filter(|r| r.to_table == "mq_ee_parent")
        .map(|r| {
            (
                r.from_table.clone(),
                r.from_column.clone(),
                r.to_column.clone(),
            )
        })
        .collect();
    ours.sort();
    assert_eq!(
        ours,
        vec![
            ("mq_ee_child_a".into(), "x".into(), "a".into()),
            ("mq_ee_child_a".into(), "y".into(), "b".into()),
            ("mq_ee_child_b".into(), "p".into(), "b".into()),
            ("mq_ee_child_b".into(), "q".into(), "a".into()),
        ],
        "one edge per column pair, by position"
    );

    // The description is cached until DDL runs.
    let describe = || DescribeTableRequest {
        session_id: sid.into(),
        table: "mq_ee_parent".into(),
        schema: Some("public".into()),
        database: None,
    };
    let before = mgr.describe_table(describe()).await.expect("describe");
    assert_eq!(before.len(), 2);
    write(&mgr, sid, "ALTER TABLE mq_ee_parent ADD COLUMN c text").await;
    let after = mgr.describe_table(describe()).await.expect("describe");
    assert_eq!(after.len(), 3, "DDL invalidates the cached description");

    write(
        &mgr,
        sid,
        "DROP TABLE mq_ee_child_a, mq_ee_child_b, mq_ee_parent",
    )
    .await;
    let _ = mgr.close_session(sid).await;
}

#[tokio::test]
#[ignore = "needs local MySQL"]
async fn mysql_locators_typed_keys_transaction_export_and_streaming() {
    let mgr = ConnectionManager::bootstrap();
    let sid = "mq_ee_my";
    let tab = "mq_ee_tab_my";
    mgr.open_session(open("mysql", sid)).await.expect("open");
    write(
        &mgr,
        sid,
        "DROP TABLE IF EXISTS mq_ee_docs; \
         CREATE TABLE mq_ee_docs (id BIGINT UNSIGNED PRIMARY KEY, body LONGTEXT); \
         INSERT INTO mq_ee_docs VALUES \
           (18446744073709551610, CONCAT(REPEAT('x', 50000), 'ROW-A')), \
           (18446744073709551611, CONCAT(REPEAT('x', 50000), 'ROW-B'))",
    )
    .await;
    let sql = "SELECT id, body FROM mq_ee_docs WHERE id = 18446744073709551611";
    let batch = grid_read(&mgr, sid, sql).await;
    let rows = &batch.results[0].result.as_ref().expect("rows").rows;
    let locator = locator_of(&rows[0][1]).expect("mysql locator");
    assert_eq!(locator.table, "mq_ee_docs");
    assert_eq!(locator.database.as_deref(), Some("mystique"));
    let text = load_all(&mgr, sid, None, &locator).await;
    assert!(text.ends_with("ROW-B"), "loaded the wrong row");

    // Uncommitted rows export through the tab.
    mgr.pin_tab(sid, tab).await.expect("pin");
    mgr.execute_pinned_batch(
        req(
            sid,
            "INSERT INTO mq_ee_docs VALUES (5, 'UNCOMMITTED')",
            QueryMode::Write,
        ),
        tab.into(),
        None,
    )
    .await
    .expect("insert");
    let through = temp_file("my_txn.csv");
    let done = mgr
        .export_object(
            export_req(
                sid,
                "mq_ee_docs",
                None,
                &through,
                ExportFormat::Csv,
                Some(tab),
            ),
            None,
        )
        .await
        .expect("export through tab");
    assert_eq!(done.row_count, 3);
    let outside = temp_file("my_pool.csv");
    let done = mgr
        .export_object(
            export_req(sid, "mq_ee_docs", None, &outside, ExportFormat::Csv, None),
            None,
        )
        .await
        .expect("export from pool");
    assert_eq!(done.row_count, 2);
    mgr.rollback_tab(tab).await.expect("rollback");

    // 200k rows streamed.
    write(
        &mgr,
        sid,
        "DROP TABLE IF EXISTS mq_ee_big; \
         CREATE TABLE mq_ee_big (id INT PRIMARY KEY, name VARCHAR(64), payload TEXT); \
         SET SESSION cte_max_recursion_depth = 300000; \
         INSERT INTO mq_ee_big \
           WITH RECURSIVE g(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM g WHERE n < 200000) \
           SELECT n, CONCAT('name ', n), CONCAT(REPEAT('abcdefgh', 24), n) FROM g",
    )
    .await;
    let path = temp_file("my_big.csv");
    let before = rss_kb();
    let peak = Arc::new(AtomicU64::new(before));
    let progress = {
        let peak = Arc::clone(&peak);
        RowProgress(Arc::new(move |_| {
            peak.fetch_max(rss_kb(), Ordering::SeqCst);
        }))
    };
    let started = Instant::now();
    let done = mgr
        .export_object(
            export_req(sid, "mq_ee_big", None, &path, ExportFormat::Csv, None),
            Some(progress),
        )
        .await
        .expect("export");
    let elapsed = started.elapsed();
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let peak = peak.load(Ordering::SeqCst);
    eprintln!(
        "mysql export csv: {} rows, {} MB file, {:.2}s ({:.0} rows/s), rss {} MB -> peak {} MB",
        done.row_count,
        size / (1024 * 1024),
        elapsed.as_secs_f64(),
        done.row_count as f64 / elapsed.as_secs_f64(),
        before / 1024,
        peak / 1024
    );
    assert_eq!(done.row_count, 200_000);
    assert!(peak.saturating_sub(before) < size / 1024 / 2);

    write(&mgr, sid, "DROP TABLE mq_ee_big; DROP TABLE mq_ee_docs").await;
    for file in [through, outside, path] {
        let _ = std::fs::remove_file(file);
    }
    let _ = mgr.close_session(sid).await;
}
