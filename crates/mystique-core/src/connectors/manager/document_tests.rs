//! Run all on a MongoDB buffer with several shell statements. The sessions
//! here are doubles; the live driver path is in the plugin's ignored tests.

use std::sync::{Arc, Mutex};

use mystique_connector_api::{
    BatchEvent, BatchPhase, ConnectorError, ConnectorResult, ConnectorSession, EngineCapabilities,
    ExecuteQueryRequest, PinnedSession, PoolStatus, ProgressPump, QueryBatchResult, QueryMode,
    QueryResult, StatementKind, StatementResult, ValidatedBatch, ValidatedQuery,
};

use super::history::QueryHistoryWriter;
use super::{ConnectionManager, LiveSession, SessionOrigin};
use crate::domain::AppendHistoryInput;

/// Index, range start, range end and text of one pinned call.
type Pinned = (u32, u32, u32, String);

/// Records what ran. A statement naming `boom` fails, `cancel` cancels the
/// run as it returns, and `hang` waits for a cancel.
#[derive(Clone, Default)]
struct Probe {
    ran: Arc<Mutex<Vec<String>>>,
    /// Statements the pin got, one batch each.
    pinned: Arc<Mutex<Vec<Pinned>>>,
}

impl Probe {
    fn ran(&self) -> Vec<String> {
        self.ran.lock().expect("ran").clone()
    }

    async fn run(&self, query: &ValidatedQuery) -> ConnectorResult<QueryResult> {
        self.ran.lock().expect("ran").push(query.text.clone());
        if query.text.contains("boom") {
            return Err(ConnectorError::Query("collection boom is on fire".into()));
        }
        if query.text.contains("cancel") {
            query.control.cancel();
        }
        if query.text.contains("hang") {
            query.control.token.cancelled().await;
            return Err(ConnectorError::Query("operation was interrupted".into()));
        }
        Ok(QueryResult {
            engine: "mongodb".into(),
            mode: query.mode,
            columns: Vec::new(),
            rows: Vec::new(),
            row_count: 0,
            truncated: false,
            duration_ms: 0,
            message: Some(query.text.clone()),
        })
    }
}

#[async_trait::async_trait]
impl ConnectorSession for Probe {
    fn engine_id(&self) -> &str {
        "mongodb"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::mongodb_defaults()
    }

    async fn ping(&self) -> ConnectorResult<()> {
        Ok(())
    }

    fn pool_status(&self) -> Option<PoolStatus> {
        None
    }

    async fn close(&self) -> ConnectorResult<()> {
        Ok(())
    }

    async fn execute_query(&self, query: &ValidatedQuery) -> ConnectorResult<QueryResult> {
        self.run(query).await
    }

    async fn begin_pinned(&self) -> ConnectorResult<Box<dyn PinnedSession + Send>> {
        Ok(Box::new(ProbePin {
            probe: self.clone(),
            open: false,
        }))
    }
}

/// Runs the first statement of each batch, as `MongoPin` does.
struct ProbePin {
    probe: Probe,
    open: bool,
}

#[async_trait::async_trait]
impl PinnedSession for ProbePin {
    async fn execute(&mut self, batch: &ValidatedBatch) -> ConnectorResult<QueryBatchResult> {
        assert_eq!(batch.statements.len(), 1, "one statement per pinned call");
        let stmt = &batch.statements[0];
        self.probe.pinned.lock().expect("pinned").push((
            stmt.index,
            stmt.sql_range.start,
            stmt.sql_range.end,
            stmt.sql.clone(),
        ));
        if stmt.kind != StatementKind::Read {
            self.open = true;
        }
        let query = ValidatedQuery {
            text: stmt.sql.clone(),
            mode: batch.mode,
            max_rows: batch.max_rows,
            timeout_ms: batch.timeout_ms,
            collection: batch.collection.clone(),
            database: batch.database.clone(),
            control: Arc::clone(&batch.control),
            params: Vec::new(),
        };
        let result = self.probe.run(&query).await?;
        Ok(QueryBatchResult {
            results: vec![StatementResult::success(stmt, 0, result, None, Vec::new())],
            stopped_at: None,
        })
    }

    async fn commit(&mut self) -> ConnectorResult<()> {
        self.open = false;
        Ok(())
    }

    async fn rollback(&mut self) -> ConnectorResult<()> {
        self.open = false;
        Ok(())
    }

    fn in_transaction(&self) -> bool {
        self.open
    }
}

#[derive(Default)]
struct History(Mutex<Vec<AppendHistoryInput>>);

impl QueryHistoryWriter for History {
    fn append_queries(&self, rows: Vec<AppendHistoryInput>) {
        self.0.lock().expect("history").extend(rows);
    }
}

async fn mongo(mgr: &ConnectionManager, session_id: &str, environment: &str) -> Probe {
    let probe = Probe::default();
    mgr.sessions.write().await.insert(
        session_id.into(),
        LiveSession {
            session: Arc::new(probe.clone()),
            display_name: "Mongo".into(),
            opened_at: String::new(),
            product_id: "mongodb".into(),
            _tunnel: None,
            environment: environment.into(),
            read_only: false,
            connection_name: "Mongo".into(),
            origin: SessionOrigin::Ui,
            profile_id: None,
            default_database: None,
            last_used: std::sync::Mutex::new(std::time::Instant::now()),
        },
    );
    probe
}

fn req(session_id: &str, text: &str, mode: QueryMode) -> ExecuteQueryRequest {
    ExecuteQueryRequest {
        session_id: session_id.into(),
        text: text.into(),
        mode,
        max_rows: Some(50),
        timeout_ms: Some(5_000),
        confirm_object_name: None,
        collection: None,
        database: None,
        execution_id: None,
        params: Vec::new(),
    }
}

fn pump() -> (ProgressPump, Arc<Mutex<Vec<BatchEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let pump = ProgressPump::new(move |event| sink.lock().expect("events").push(event));
    (pump, events)
}

fn slice<'a>(text: &'a str, stmt: &StatementResult) -> &'a str {
    &text[stmt.sql_range.start as usize..stmt.sql_range.end as usize]
}

const SCRIPT: &str = "// three statements\ndb.a.find({})\ndb.b\n  .insertOne({ x: \"1;2\" });\n\ndb.c.find({}).limit(2)";

#[tokio::test]
async fn each_statement_runs_in_order_with_its_own_result() {
    let mgr = ConnectionManager::bootstrap();
    let history = Arc::new(History::default());
    mgr.set_query_history_writer(Arc::clone(&history) as Arc<dyn QueryHistoryWriter>);
    let probe = mongo(&mgr, "local-mongo", "local").await;
    let (progress, events) = pump();

    let batch = mgr
        .execute_batch(req("local-mongo", SCRIPT, QueryMode::Write), Some(progress))
        .await
        .expect("batch");

    let want = [
        "// three statements\ndb.a.find({})",
        "db.b\n  .insertOne({ x: \"1;2\" })",
        "db.c.find({}).limit(2)",
    ];
    assert_eq!(probe.ran(), want);
    assert_eq!(batch.stopped_at, None);
    assert_eq!(batch.results.len(), 3);
    for (i, stmt) in batch.results.iter().enumerate() {
        assert_eq!(stmt.index as usize, i);
        assert_eq!(slice(SCRIPT, stmt), want[i]);
        assert!(stmt.error.is_none());
        assert_eq!(
            stmt.result.as_ref().and_then(|r| r.message.as_deref()),
            Some(want[i])
        );
    }
    let kinds: Vec<_> = batch.results.iter().map(|stmt| stmt.kind).collect();
    assert_eq!(
        kinds,
        [
            StatementKind::Read,
            StatementKind::Write,
            StatementKind::Read
        ]
    );

    // The pump sends the first event and coalesces the rest of a fast batch;
    // the result supersedes what it still held.
    let events = events.lock().expect("events").clone();
    assert_eq!(
        events.first().map(|e| (e.index, e.total, e.phase)),
        Some((0, 3, BatchPhase::Started))
    );
    assert!(events.iter().all(|e| e.total == 3), "{events:?}");

    let rows = history.0.lock().expect("history").clone();
    let bodies: Vec<_> = rows.iter().map(|row| row.body.as_str()).collect();
    assert_eq!(bodies, want, "one history row per statement, as SQL");
    assert!(rows.iter().all(|row| row.success));
}

#[tokio::test]
async fn the_first_error_stops_the_batch_and_points_at_its_statement() {
    let mgr = ConnectionManager::bootstrap();
    let history = Arc::new(History::default());
    mgr.set_query_history_writer(Arc::clone(&history) as Arc<dyn QueryHistoryWriter>);
    let probe = mongo(&mgr, "local-mongo", "local").await;
    let text = "db.a.find({})\n  db.boom.find({})\ndb.c.find({})";

    let batch = mgr
        .execute_batch(req("local-mongo", text, QueryMode::Write), None)
        .await
        .expect("a statement error is a result, not an Err");

    assert_eq!(probe.ran(), ["db.a.find({})", "db.boom.find({})"]);
    assert_eq!(batch.stopped_at, Some(1));
    assert_eq!(batch.results.len(), 2);
    let failed = &batch.results[1];
    assert_eq!(slice(text, failed), "db.boom.find({})");
    assert!(failed
        .error
        .as_deref()
        .is_some_and(|e| e.contains("on fire")));
    let at = failed.error_position.as_ref().expect("position");
    assert_eq!(at.offset as usize, text.find("db.boom").expect("boom"));
    assert_eq!((at.line, at.column), (2, 3));

    let rows = history.0.lock().expect("history").clone();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].success && !rows[1].success);

    // `execute_query` (agent, MCP) folds the stop into an error.
    let err = mgr
        .execute_query(req("local-mongo", text, QueryMode::Write))
        .await
        .expect_err("folded");
    assert!(err.to_string().contains("on fire"), "{err}");
}

#[tokio::test]
async fn read_mode_refuses_a_write_anywhere_before_anything_runs() {
    let mgr = ConnectionManager::bootstrap();
    let probe = mongo(&mgr, "local-mongo", "local").await;
    let text = "db.a.find({})\ndb.a.deleteMany({})\ndb.c.find({})";

    let err = mgr
        .execute_batch(req("local-mongo", text, QueryMode::Read), None)
        .await
        .expect_err("read mode");
    assert!(probe.ran().is_empty(), "nothing ran: {:?}", probe.ran());
    let ConnectorError::QueryAt {
        message,
        position,
        statement_index,
    } = &err
    else {
        panic!("expected a positioned error, got {err:?}");
    };
    assert!(message.starts_with("policy: Statement 2"), "{message}");
    assert!(message.contains("deleteMany"), "{message}");
    assert!(message.ends_with("Nothing ran."), "{message}");
    assert_eq!(*statement_index, Some(1));
    assert_eq!((position.line, position.column), (2, 1));

    // A live read-only session reads the same way, whatever the client sent.
    mgr.sessions
        .write()
        .await
        .get_mut("local-mongo")
        .expect("session")
        .read_only = true;
    let err = mgr
        .execute_batch(req("local-mongo", text, QueryMode::Write), None)
        .await
        .expect_err("read-only session");
    assert!(err.to_string().contains("Statement 2"), "{err}");
    assert!(probe.ran().is_empty());
}

#[tokio::test]
async fn a_statement_the_executor_cannot_run_stops_the_batch_before_it_starts() {
    let mgr = ConnectionManager::bootstrap();
    let probe = mongo(&mgr, "local-mongo", "local").await;
    let text = "db.a.insertOne({ a: 1 })\ndb.a.frobnicate()";

    let err = mgr
        .execute_batch(req("local-mongo", text, QueryMode::Write), None)
        .await
        .expect_err("unsupported method");
    assert!(probe.ran().is_empty(), "the insert must not land");
    let msg = err.to_string();
    assert!(msg.starts_with("Statement 2 (db.a.frobnicate())"), "{msg}");
    assert!(msg.ends_with("Nothing ran."), "{msg}");
    assert_eq!(
        err.local_position().map(|p| p.offset as usize),
        text.find("db.a.frobnicate")
    );
}

#[tokio::test]
async fn one_guard_prompt_covers_every_statement_at_the_worst_risk() {
    let mgr = ConnectionManager::bootstrap();
    let probe = mongo(&mgr, "prod-mongo", "production").await;
    let text = "db.a.find({})\ndb.a.insertOne({ a: 1 })\ndb.a.deleteMany({})";

    let err = mgr
        .execute_batch(req("prod-mongo", text, QueryMode::Write), None)
        .await
        .expect_err("production write prompts");
    let ConnectorError::WriteGuard(prompt) = &err else {
        panic!("expected the write guard, got {err:?}");
    };
    assert_eq!(
        prompt.statements,
        [
            "db.a.find({})",
            "db.a.insertOne({ a: 1 })",
            "db.a.deleteMany({})"
        ]
    );
    assert_eq!(prompt.preview, "find; insertone; deletemany");
    assert!(probe.ran().is_empty(), "nothing runs before the prompt");

    // Reads alone never prompt, even on production.
    let batch = mgr
        .execute_batch(
            req(
                "prod-mongo",
                "db.a.find({})\ndb.b.find({})",
                QueryMode::Write,
            ),
            None,
        )
        .await
        .expect("reads");
    assert_eq!(batch.results.len(), 2);
}

#[tokio::test]
async fn a_destructive_statement_raises_the_whole_batch() {
    let mgr = ConnectionManager::bootstrap();
    mongo(&mgr, "prod-mongo", "production").await;
    let read = "db.a.find({})";
    let drop = r#"db.orders.aggregate([{ $match: {} }, { $drop: "t" }])"#;
    let text = format!("{read}\n{drop}");
    let one = match mgr
        .execute_batch(req("prod-mongo", drop, QueryMode::Write), None)
        .await
    {
        Err(ConnectorError::WriteGuard(prompt)) => prompt.kind,
        other => panic!("expected the write guard, got {other:?}"),
    };
    let both = match mgr
        .execute_batch(req("prod-mongo", &text, QueryMode::Write), None)
        .await
    {
        Err(ConnectorError::WriteGuard(prompt)) => prompt.kind,
        other => panic!("expected the write guard, got {other:?}"),
    };
    assert_eq!(one, both, "a read in front does not lower the prompt");
    let write = match mgr
        .execute_batch(
            req("prod-mongo", "db.a.insertOne({ a: 1 })", QueryMode::Write),
            None,
        )
        .await
    {
        Err(ConnectorError::WriteGuard(prompt)) => prompt.kind,
        other => panic!("expected the write guard, got {other:?}"),
    };
    assert_ne!(both, write, "the batch is asked about at Destructive");
}

#[tokio::test]
async fn cancel_between_statements_stops_the_rest() {
    let mgr = ConnectionManager::bootstrap();
    let probe = mongo(&mgr, "local-mongo", "local").await;
    let text = "db.cancel.find({})\ndb.b.insertOne({ a: 1 })";

    let err = mgr
        .execute_batch(req("local-mongo", text, QueryMode::Write), None)
        .await
        .expect_err("cancelled");
    assert!(err.to_string().contains("cancelled"), "{err}");
    assert_eq!(probe.ran(), ["db.cancel.find({})"]);
}

#[tokio::test]
async fn cancel_inside_a_statement_stops_the_rest() {
    let mgr = Arc::new(ConnectionManager::bootstrap());
    let probe = mongo(&mgr, "local-mongo", "local").await;
    let text = "db.a.find({})\ndb.hang.find({})\ndb.c.insertOne({ a: 1 })";
    let mut request = req("local-mongo", text, QueryMode::Write);
    request.execution_id = Some("run-hang".into());

    let canceller = Arc::clone(&mgr);
    let cancel = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        canceller.cancel_query("run-hang").await
    });
    let outcome = mgr.execute_batch(request, None).await;
    cancel.await.expect("join").expect("cancel");
    outcome.expect_err("cancelled inside the second statement");
    assert_eq!(probe.ran(), ["db.a.find({})", "db.hang.find({})"]);
}

#[tokio::test]
async fn one_statement_runs_as_typed() {
    let mgr = ConnectionManager::bootstrap();
    let probe = mongo(&mgr, "local-mongo", "local").await;
    let text = "// users\ndb.users.find({});";

    let batch = mgr
        .execute_batch(req("local-mongo", text, QueryMode::Write), None)
        .await
        .expect("one statement");
    assert_eq!(probe.ran(), [text], "the whole text, as before");
    assert_eq!(batch.results.len(), 1);
    assert_eq!(slice(text, &batch.results[0]), text);

    // Its error is still the run's error, not a stopped batch.
    let err = mgr
        .execute_batch(
            req("local-mongo", "db.boom.find({})", QueryMode::Write),
            None,
        )
        .await
        .expect_err("one failing statement");
    assert!(err.to_string().contains("on fire"), "{err}");
}

#[tokio::test]
async fn a_manual_commit_tab_runs_each_statement_on_its_pin() {
    let mgr = ConnectionManager::bootstrap();
    let probe = mongo(&mgr, "local-mongo", "local").await;
    let text = "db.a.find({})\ndb.a.insertOne({ a: 1 })\ndb.boom.find({})\ndb.c.find({})";
    let (progress, events) = pump();

    let batch = mgr
        .execute_pinned_batch(
            req("local-mongo", text, QueryMode::Write),
            "tab-1".into(),
            Some(progress),
        )
        .await
        .expect("pinned batch");

    let pinned = probe.pinned.lock().expect("pinned").clone();
    let got: Vec<_> = pinned
        .iter()
        .map(|(index, start, end, sql)| {
            assert_eq!(&text[*start as usize..*end as usize], sql);
            (*index, sql.as_str())
        })
        .collect();
    assert_eq!(
        got,
        [
            (0, "db.a.find({})"),
            (1, "db.a.insertOne({ a: 1 })"),
            (2, "db.boom.find({})"),
        ]
    );
    assert_eq!(batch.stopped_at, Some(2));
    assert_eq!(batch.results.len(), 3);
    assert_eq!(
        batch.results[2].error_position.as_ref().map(|p| p.line),
        Some(3)
    );
    assert!(mgr.pin_status("tab-1").await.in_transaction);
    let first = events.lock().expect("events").first().cloned();
    assert_eq!(
        first.map(|e| (e.index, e.total, e.phase)),
        Some((0, 4, BatchPhase::Started))
    );
    let _ = mgr.rollback_tab("tab-1").await;
}
