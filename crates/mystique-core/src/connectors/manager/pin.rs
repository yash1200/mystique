//! Tab → pinned connection. Idle pins roll back. Closing the tab rolls back.
//!
//! The first write on a pinned tab opens a transaction on every engine; the
//! plugins do that. A transaction lost without the user ending it (idle
//! reap, disconnect, a server-side rollback) is reported as `rolled_back`,
//! and the tab's next commit or run is refused once.

use std::sync::Arc;
use std::time::{Duration, Instant};

use mystique_connector_api::{
    clamp_timeout_ms, ConnectorError, ConnectorResult, ProgressPump, WriteAction,
};
use mystique_connector_api::{
    gate_sql_batch_in, PinStatus, PinnedSession, PlannedStatement, QueryBatchResult, QueryMode,
    SqlDialect, ValidatedBatch, DEFAULT_MAX_ROWS, HARD_MAX_ROWS,
};
use tokio::sync::Mutex as AsyncMutex;

use super::guard::WriteCheck;
use super::ConnectionManager;

pub(super) const PIN_IDLE: Duration = Duration::from_secs(5 * 60);
pub(super) const MCP_IDLE: Duration = Duration::from_secs(10 * 60);

const ROLLED_BACK: &str = "This tab's open transaction was rolled back after it sat idle or \
     its connection closed. Nothing in it was committed.";

pub(super) struct TabPin {
    session_id: String,
    pin: AsyncMutex<Box<dyn PinnedSession + Send>>,
    idle: Duration,
    last_used: std::sync::Mutex<Instant>,
    /// Exports and value reads running on this pin. A busy pin is never idle:
    /// a long export must not have its transaction reaped under it.
    busy: std::sync::atomic::AtomicUsize,
}

impl TabPin {
    fn touch(&self) {
        if let Ok(mut slot) = self.last_used.lock() {
            *slot = Instant::now();
        }
    }

    fn expired(&self) -> bool {
        if self.busy.load(std::sync::atomic::Ordering::SeqCst) > 0 {
            return false;
        }
        self.last_used
            .lock()
            .map(|slot| slot.elapsed() >= self.idle)
            .unwrap_or(true)
    }
}

impl ConnectionManager {
    pub(super) async fn gate_for(&self, key: &str) -> Arc<AsyncMutex<()>> {
        let mut gates = self.open_gates.lock().await;
        if let Some(gate) = gates.get(key) {
            return Arc::clone(gate);
        }
        let gate = Arc::new(AsyncMutex::new(()));
        gates.insert(key.to_string(), Arc::clone(&gate));
        gate
    }

    /// Serialize check-then-open so two callers for one id build one pool.
    pub(crate) async fn get_or_open(
        &self,
        req: super::OpenSessionRequest,
        origin: super::SessionOrigin,
    ) -> ConnectorResult<super::SessionInfo> {
        let Some(id) = req.session_id.clone().filter(|value| !value.is_empty()) else {
            return self.open_session_as(req, origin).await;
        };
        let gate = self.gate_for(&id).await;
        let _hold = gate.lock().await;
        if let Some(info) = self.get_session(&id).await {
            self.touch_session(&id);
            return Ok(info);
        }
        self.open_session_as(req, origin).await
    }

    /// True when the pool is already open. Callers skip binding secrets then.
    pub(crate) async fn reuse_live(&self, session_id: &str) -> bool {
        if self.get_session(session_id).await.is_none() {
            return false;
        }
        self.touch_session(session_id);
        true
    }

    pub async fn pin_tab(&self, session_id: &str, tab_id: &str) -> ConnectorResult<PinStatus> {
        self.pin_tab_for(session_id, tab_id, PIN_IDLE).await
    }

    pub async fn pin_tab_for(
        &self,
        session_id: &str,
        tab_id: &str,
        idle: Duration,
    ) -> ConnectorResult<PinStatus> {
        let tab_id = tab_id.trim();
        let session_id = session_id.trim();
        if tab_id.is_empty() || session_id.is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "A pinned tab needs a session and a tab id.".into(),
            ));
        }
        // A tab's transaction is the human's; tool sessions never pin (F-01).
        self.refuse_tool_session(session_id, "pinned to a tab")
            .await?;
        let gate = self.gate_for(&format!("pin:{tab_id}")).await;
        let _hold = gate.lock().await;
        if let Some(existing) = self.pins.lock().await.get(tab_id).cloned() {
            if existing.session_id == session_id {
                existing.touch();
                let pin = existing.pin.lock().await;
                return Ok(PinStatus::pinned(pin.in_transaction()));
            }
        }
        drop(_hold);
        let _ = self.rollback_tab(tab_id).await;
        let gate = self.gate_for(&format!("pin:{tab_id}")).await;
        let _hold = gate.lock().await;
        let session = self.session(session_id).await?;
        let boxed = session.begin_pinned().await?;
        let tab_pin = Arc::new(TabPin {
            session_id: session_id.to_string(),
            pin: AsyncMutex::new(boxed),
            idle,
            last_used: std::sync::Mutex::new(Instant::now()),
            busy: std::sync::atomic::AtomicUsize::new(0),
        });
        self.pins
            .lock()
            .await
            .insert(tab_id.to_string(), Arc::clone(&tab_pin));
        let pin = tab_pin.pin.lock().await;
        Ok(PinStatus::pinned(pin.in_transaction()))
    }

    pub async fn pin_status(&self, tab_id: &str) -> PinStatus {
        let rolled_back = self.rolled_back.lock().await.contains(tab_id);
        let Some(tab) = self.pins.lock().await.get(tab_id).cloned() else {
            return PinStatus {
                rolled_back,
                ..PinStatus::idle()
            };
        };
        let pin = tab.pin.lock().await;
        PinStatus {
            rolled_back,
            ..PinStatus::pinned(pin.in_transaction())
        }
    }

    pub async fn commit_tab(&self, tab_id: &str) -> ConnectorResult<PinStatus> {
        self.end_tab(tab_id, true).await
    }

    /// Roll back the open transaction and keep the pinned connection.
    pub async fn rollback_open_tab(&self, tab_id: &str) -> ConnectorResult<PinStatus> {
        self.end_tab(tab_id, false).await
    }

    /// Unpin. `rolled_back` says whether an open transaction was discarded.
    pub async fn rollback_tab(&self, tab_id: &str) -> ConnectorResult<PinStatus> {
        self.rolled_back.lock().await.remove(tab_id);
        let Some(tab) = self.pins.lock().await.remove(tab_id) else {
            return Ok(PinStatus::idle());
        };
        let mut pin = tab.pin.lock().await;
        let open = pin.in_transaction();
        if open {
            self.meta.invalidate(&tab.session_id);
        }
        pin.rollback().await?;
        Ok(PinStatus {
            rolled_back: open,
            ..PinStatus::idle()
        })
    }

    /// Unpin without the user asking. A discarded transaction is remembered
    /// so the tab's next commit or run says so instead of starting fresh.
    async fn drop_tab(&self, tab_id: &str) {
        let discarded = !matches!(
            self.rollback_tab(tab_id).await,
            Ok(PinStatus {
                rolled_back: false,
                ..
            })
        );
        if discarded {
            self.rolled_back.lock().await.insert(tab_id.to_string());
        }
    }

    /// Whether `drop_tab` discarded this tab's transaction. Reported once.
    async fn take_rolled_back(&self, tab_id: &str) -> bool {
        self.rolled_back.lock().await.remove(tab_id)
    }

    async fn end_tab(&self, tab_id: &str, commit: bool) -> ConnectorResult<PinStatus> {
        let lost = self.take_rolled_back(tab_id).await;
        let Some(tab) = self.pins.lock().await.get(tab_id).cloned() else {
            if commit && lost {
                return Err(ConnectorError::Query(ROLLED_BACK.into()));
            }
            return Ok(PinStatus::idle());
        };
        tab.touch();
        if commit && lost && !tab.pin.lock().await.in_transaction() {
            return Err(ConnectorError::Query(ROLLED_BACK.into()));
        }
        let mut pin = tab.pin.lock().await;
        // DDL in the transaction becomes visible to the pool, or is undone.
        self.meta.invalidate(&tab.session_id);
        if commit {
            pin.commit().await?;
        } else {
            pin.rollback().await?;
        }
        Ok(PinStatus::pinned(pin.in_transaction()))
    }

    pub async fn rollback_all_pins(&self) {
        let tabs: Vec<String> = self.pins.lock().await.keys().cloned().collect();
        for tab in tabs {
            let _ = self.rollback_tab(&tab).await;
        }
    }

    pub async fn rollback_pins_for_session(&self, session_id: &str) {
        let tabs: Vec<String> = self
            .pins
            .lock()
            .await
            .iter()
            .filter(|(_, pin)| pin.session_id == session_id)
            .map(|(tab, _)| tab.clone())
            .collect();
        for tab in tabs {
            self.drop_tab(&tab).await;
        }
    }

    pub async fn reap_idle(&self) {
        let stale: Vec<String> = self
            .pins
            .lock()
            .await
            .iter()
            .filter(|(_, pin)| pin.expired())
            .map(|(tab, _)| tab.clone())
            .collect();
        for tab in stale {
            self.drop_tab(&tab).await;
        }
        self.reap_mcp_sessions().await;
    }

    async fn reap_mcp_sessions(&self) {
        let stale: Vec<String> = {
            let guard = self.sessions.read().await;
            guard
                .iter()
                .filter(|(id, live)| {
                    live.origin == super::SessionOrigin::Mcp
                        && live
                            .last_used
                            .lock()
                            .map(|slot| slot.elapsed() >= MCP_IDLE)
                            .unwrap_or(false)
                        && !self.session_has_pin(id)
                })
                .map(|(id, _)| id.clone())
                .collect()
        };
        for id in stale {
            if self.session_busy(&id).await {
                continue;
            }
            let _ = self.close_session(&id).await;
        }
    }

    fn session_has_pin(&self, session_id: &str) -> bool {
        // Called under the sessions read lock. The pin map is a different lock.
        // try_lock avoids waiting while that read lock is held.
        let Ok(pins) = self.pins.try_lock() else {
            return true;
        };
        pins.values().any(|pin| pin.session_id == session_id)
    }

    pub(super) fn touch_session(&self, session_id: &str) {
        if let Ok(guard) = self.sessions.try_read() {
            if let Some(live) = guard.get(session_id) {
                if let Ok(mut slot) = live.last_used.lock() {
                    *slot = Instant::now();
                }
            }
        }
    }

    pub async fn execute_pinned_batch(
        &self,
        req: mystique_connector_api::ExecuteQueryRequest,
        tab_id: String,
        progress: Option<ProgressPump>,
    ) -> ConnectorResult<QueryBatchResult> {
        let tab_id = tab_id.trim();
        if tab_id.is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "A manual-commit query needs a tab id.".into(),
            ));
        }
        self.ensure_tab_pin(&req.session_id, tab_id).await?;
        self.execute_batch_in(req, progress, Some(tab_id.to_string()))
            .await
    }

    pub(super) async fn ensure_tab_pin(
        &self,
        session_id: &str,
        tab_id: &str,
    ) -> ConnectorResult<PinStatus> {
        // A fresh pin, or a fresh transaction on this one, would run these
        // statements outside the lost transaction.
        if self.take_rolled_back(tab_id).await {
            return Err(ConnectorError::Query(format!(
                "{ROLLED_BACK} Nothing ran. Run again to continue without it."
            )));
        }
        if self.pin_status(tab_id).await.pinned {
            let pins = self.pins.lock().await;
            if let Some(pin) = pins.get(tab_id) {
                if pin.session_id == session_id {
                    pin.touch();
                    let guard = pin.pin.lock().await;
                    return Ok(PinStatus::pinned(guard.in_transaction()));
                }
            }
        }
        self.pin_tab(session_id, tab_id).await
    }

    pub(super) async fn run_on_pin(
        &self,
        tab_id: &str,
        batch: &ValidatedBatch,
    ) -> ConnectorResult<QueryBatchResult> {
        let Some(tab) = self.pins.lock().await.get(tab_id).cloned() else {
            return Err(ConnectorError::InvalidConfig(
                "This tab has no pinned connection.".into(),
            ));
        };
        tab.touch();
        let mut pin = tab.pin.lock().await;
        let was_open = pin.in_transaction();
        let result = pin.execute(batch).await;
        // Open before, closed after, and the batch failed without a COMMIT or
        // ROLLBACK of its own: the server or the connection dropped it.
        let failed = result.as_ref().map_or(true, |ran| {
            ran.results.iter().any(|stmt| stmt.error.is_some())
        });
        let ended_by_user = batch.statements.iter().any(|stmt| stmt.txn_boundary);
        if was_open && failed && !ended_by_user && !pin.in_transaction() {
            self.rolled_back.lock().await.insert(tab_id.to_string());
        }
        result
    }

    /// One Mongo shell statement on the tab's pin, already validated and
    /// guarded. A buffer of several calls this once per statement, in order.
    // Values the caller already resolved from the request.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn pinned_document_batch(
        &self,
        session_id: &str,
        tab_id: &str,
        statement: PlannedStatement,
        mode: QueryMode,
        max_rows: u32,
        timeout_ms: u64,
        collection: Option<String>,
        database: Option<String>,
        control: Arc<mystique_connector_api::QueryControl>,
    ) -> ConnectorResult<QueryBatchResult> {
        self.ensure_tab_pin(session_id, tab_id).await?;
        let batch = ValidatedBatch {
            statements: vec![statement],
            mode,
            max_rows,
            timeout_ms,
            explain_executes: false,
            collection,
            database,
            control,
            progress: None,
        };
        self.run_on_pin(tab_id, &batch).await
    }
}

/// A tab's pinned connection, borrowed for an export or a value read.
pub(super) struct PinRead {
    tab: Arc<TabPin>,
}

impl PinRead {
    /// Stream through the tab's open transaction. The pin is marked busy for
    /// the whole read so the idle reaper leaves it alone.
    pub(super) async fn stream_rows(
        &self,
        source: &mystique_connector_api::RowSource,
        control: &Arc<mystique_connector_api::QueryControl>,
        sink: &mut dyn mystique_connector_api::RowSink,
    ) -> ConnectorResult<()> {
        use std::sync::atomic::Ordering;
        self.tab.busy.fetch_add(1, Ordering::SeqCst);
        self.tab.touch();
        let result = {
            let mut pin = self.tab.pin.lock().await;
            pin.stream_rows(source, control, sink).await
        };
        self.tab.touch();
        self.tab.busy.fetch_sub(1, Ordering::SeqCst);
        result
    }
}

impl ConnectionManager {
    /// The tab's pin when it belongs to `session_id` and has an open
    /// transaction. Without one there is nothing uncommitted to see, and the
    /// pool reads without holding up the tab.
    pub(super) async fn pinned_for_read(
        &self,
        session_id: &str,
        tab_id: Option<&str>,
    ) -> Option<PinRead> {
        let tab_id = tab_id?.trim();
        if tab_id.is_empty() {
            return None;
        }
        let tab = self.pins.lock().await.get(tab_id).cloned()?;
        if tab.session_id != session_id {
            return None;
        }
        let open = tab.pin.lock().await.in_transaction();
        open.then_some(PinRead { tab })
    }
}

impl ConnectionManager {
    /// Run a batch and store one history row per statement.
    ///
    /// The canvas, the agent, and MCP share this path. The frontend does not
    /// see MCP, so it cannot be the writer. A history failure leaves the query
    /// result unchanged.
    pub(super) async fn execute_batch_in(
        &self,
        mut req: mystique_connector_api::ExecuteQueryRequest,
        progress: Option<ProgressPump>,
        tab_id: Option<String>,
    ) -> ConnectorResult<QueryBatchResult> {
        // Register first so a cancel sent while we plan or wait is not lost.
        let run = self.begin_run(&req.session_id, req.execution_id.take());
        let control = Arc::clone(&run.control);
        let started = std::time::Instant::now();
        let req_session = req.session_id.clone();
        tracing::debug!(session_id = %req_session, pinned = tab_id.is_some(), "query run started");
        let ctx = self.history_context(&req.session_id).await;
        let text = req.text.clone();
        let database = req.database.clone();
        let outcome = if control.is_cancelled() {
            Err(super::run::cancelled_error())
        } else {
            self.execute_batch_inner(req, progress, tab_id, control)
                .await
        };
        drop(run);
        log_run(&req_session, started, &outcome);
        if let Some(ctx) = ctx {
            let rows = super::history::rows_from_outcome(
                &ctx,
                &text,
                database.as_deref(),
                super::history::elapsed_ms(started),
                &outcome,
            );
            self.persist_history(rows).await;
        }
        outcome
    }

    async fn execute_batch_inner(
        &self,
        req: mystique_connector_api::ExecuteQueryRequest,
        progress: Option<ProgressPump>,
        tab_id: Option<String>,
        control: Arc<mystique_connector_api::QueryControl>,
    ) -> ConnectorResult<QueryBatchResult> {
        let (session, live_read_only) = self.session_and_read_only(&req.session_id).await?;
        let engine = session.engine_id().to_string();
        let read_only = live_read_only;
        let mode = if read_only { QueryMode::Read } else { req.mode };
        let max_rows = req
            .max_rows
            .unwrap_or(DEFAULT_MAX_ROWS)
            .clamp(1, HARD_MAX_ROWS);
        let timeout_ms = clamp_timeout_ms(req.timeout_ms);
        self.touch_session(&req.session_id);

        if engine.eq_ignore_ascii_case("mongodb") {
            return self
                .execute_document_batch(
                    session, req, mode, max_rows, timeout_ms, read_only, progress, tab_id, control,
                )
                .await;
        }

        let plan = gate_sql_batch_in(
            &req.text,
            SqlDialect::from_engine(&engine),
            mode,
            tab_id.is_some(),
            &req.params,
        )?;
        self.enforce_write_guard(WriteCheck {
            preview: req.text.clone(),
            statements: plan
                .statements
                .iter()
                .map(|stmt| stmt.sql.clone())
                .collect(),
            object_name: plan.destructive_name.as_deref(),
            confirm_object_name: req.confirm_object_name.as_deref(),
            profile_read_only: read_only,
            pinned: tab_id.is_some(),
            request: Some(&req.text),
            ..WriteCheck::new(&req.session_id, WriteAction::Query, plan.worst.write_risk())
        })
        .await?;

        let changes_catalog = plan
            .statements
            .iter()
            .any(|stmt| super::meta_cache::may_change_catalog(&stmt.sql));
        if let Some(tab) = tab_id.as_deref() {
            self.ensure_tab_pin(&req.session_id, tab).await?;
        }

        let batch = ValidatedBatch {
            statements: plan.statements,
            mode,
            max_rows,
            timeout_ms,
            explain_executes: plan.explain_executes,
            collection: req.collection,
            database: req.database,
            control: Arc::clone(&control),
            progress,
        };
        let mut result = if let Some(tab) = tab_id.as_deref() {
            self.drive_registered(&control, timeout_ms, self.run_on_pin(tab, &batch))
                .await
        } else {
            self.drive_registered(&control, timeout_ms, session.execute_batch(&batch))
                .await
        };
        // Even a failed DDL statement may have changed something.
        if changes_catalog {
            self.meta.invalidate(&req.session_id);
        }
        if let Ok(batch_result) = result.as_mut() {
            mystique_connector_api::place_error_positions(
                &req.text,
                SqlDialect::from_engine(&engine),
                batch_result,
            );
        }
        result
    }
}

/// One line per run: timing and counts, never the SQL text or its parameters.
fn log_run(
    session_id: &str,
    started: std::time::Instant,
    outcome: &ConnectorResult<QueryBatchResult>,
) {
    let elapsed_ms = started.elapsed().as_millis() as u64;
    match outcome {
        Ok(batch) => {
            let rows: u64 = batch
                .results
                .iter()
                .filter_map(|r| r.result.as_ref().map(|q| q.row_count))
                .sum();
            let affected: u64 = batch.results.iter().filter_map(|r| r.affected_rows).sum();
            let failed = batch.results.iter().filter(|r| r.error.is_some()).count();
            tracing::info!(
                session_id,
                statements = batch.results.len(),
                rows,
                affected,
                failed,
                elapsed_ms,
                "query run finished"
            );
        }
        Err(err) => tracing::info!(
            session_id,
            error = err.kind(),
            elapsed_ms,
            "query run failed"
        ),
    }
}
