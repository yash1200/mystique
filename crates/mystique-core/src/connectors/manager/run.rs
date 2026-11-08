//! In-flight statement registry. Cancel looks up `execution_id` and fires the hook.
//!
//! The id is the client token, not the connection profile id. Each run
//! registers it before any other work. A cancel that arrives before the run
//! is kept for a minute, so the run starts already cancelled.

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mystique_connector_api::{
    ConnectorError, ConnectorResult, QueryControl, CANCEL_CLEANUP_MS, NO_TIMEOUT,
};
use tokio::sync::Semaphore;
use uuid::Uuid;

use super::ConnectionManager;

pub(super) struct Inflight {
    pub session_id: String,
    control: Arc<QueryControl>,
    /// Set for a cancel whose run has not registered yet.
    early: Option<Instant>,
}

/// Explorer and metrics only. Query execution is not limited by this.
pub(super) const METADATA_PERMITS: usize = 6;

/// How long a cancel waits for its run to register.
const EARLY_CANCEL_TTL: Duration = Duration::from_secs(60);

pub(super) fn cancelled_error() -> ConnectorError {
    ConnectorError::Query("Query cancelled.".into())
}

impl ConnectionManager {
    /// Missing and blank ids are a no-op. The button and Esc both land here.
    pub async fn cancel_query(&self, execution_id: &str) -> ConnectorResult<()> {
        let id = execution_id.trim();
        if id.is_empty() {
            return Ok(());
        }
        let control = {
            let mut running = self.running.lock().await;
            running.retain(|_, run| run.early.is_none_or(|at| at.elapsed() < EARLY_CANCEL_TTL));
            match running.get(id) {
                Some(run) => Arc::clone(&run.control),
                None => {
                    let control = QueryControl::new();
                    running.insert(
                        id.to_string(),
                        Inflight {
                            session_id: String::new(),
                            control: Arc::clone(&control),
                            early: Some(Instant::now()),
                        },
                    );
                    control
                }
            }
        };
        control.cancel();
        Ok(())
    }

    /// Call first, before history, gates, guards, or pins. A run cancelled
    /// before it registered gets a control that is already cancelled.
    pub(super) async fn begin_run(
        &self,
        session_id: &str,
        execution_id: Option<String>,
    ) -> (String, Arc<QueryControl>) {
        let id = execution_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let mut running = self.running.lock().await;
        let control = match running.remove(&id) {
            Some(prev) if prev.early.is_some() => prev.control,
            _ => QueryControl::new(),
        };
        running.insert(
            id.clone(),
            Inflight {
                session_id: session_id.to_string(),
                control: Arc::clone(&control),
                early: None,
            },
        );
        (id, control)
    }

    pub(super) async fn session_busy(&self, session_id: &str) -> bool {
        self.running
            .lock()
            .await
            .values()
            .any(|run| run.early.is_none() && run.session_id == session_id)
    }

    pub(super) async fn cancel_runs_for(&self, session_id: &str) -> bool {
        let controls: Vec<Arc<QueryControl>> = self
            .running
            .lock()
            .await
            .values()
            .filter(|run| run.early.is_none() && run.session_id == session_id)
            .map(|run| Arc::clone(&run.control))
            .collect();
        let any = !controls.is_empty();
        for control in controls {
            control.cancel();
        }
        any
    }

    pub(super) async fn finish_run(&self, id: &str) {
        self.running.lock().await.remove(id);
    }

    /// Overall deadline. On timeout, cancel (which fires the engine interrupt)
    /// and keep polling so the plugin can `RESET` / `ROLLBACK`, then return
    /// [`ConnectorError::Timeout`] rather than the driver's cancel error.
    /// [`NO_TIMEOUT`] has no deadline; cancel still stops it.
    pub(super) async fn drive_registered<T>(
        &self,
        control: &Arc<QueryControl>,
        timeout_ms: u64,
        work: impl Future<Output = ConnectorResult<T>>,
    ) -> ConnectorResult<T> {
        if control.is_cancelled() {
            return Err(cancelled_error());
        }
        let cleanup = Duration::from_millis(CANCEL_CLEANUP_MS);
        tokio::pin!(work);
        tokio::select! {
            biased;
            result = &mut work => result,
            _ = control.token.cancelled() => {
                let _ = tokio::time::timeout(cleanup, &mut work).await;
                Err(cancelled_error())
            }
            _ = tokio::time::sleep(Duration::from_millis(timeout_ms)), if timeout_ms != NO_TIMEOUT => {
                control.cancel();
                let _ = tokio::time::timeout(cleanup, &mut work).await;
                Err(ConnectorError::Timeout(format!(
                    "Query exceeded {timeout_ms} ms."
                )))
            }
        }
    }

    /// Export has no row-cap deadline. Cancel still stops the current page.
    pub(super) async fn drive_until_cancel<T>(
        &self,
        control: &Arc<QueryControl>,
        work: impl Future<Output = ConnectorResult<T>>,
    ) -> ConnectorResult<T> {
        if control.is_cancelled() {
            return Err(cancelled_error());
        }
        let cleanup = Duration::from_millis(CANCEL_CLEANUP_MS);
        tokio::pin!(work);
        tokio::select! {
            biased;
            result = &mut work => result,
            _ = control.token.cancelled() => {
                let _ = tokio::time::timeout(cleanup, &mut work).await;
                Err(cancelled_error())
            }
        }
    }

    /// Hold one metadata permit across `fut`, including the await.
    /// The semaphore is never closed.
    pub(super) async fn with_metadata<T>(
        &self,
        fut: impl Future<Output = ConnectorResult<T>>,
    ) -> ConnectorResult<T> {
        let Ok(permit) = self.metadata.acquire().await else {
            return Err(ConnectorError::Internal(
                "metadata limiter is closed".into(),
            ));
        };
        let result = fut.await;
        drop(permit);
        result
    }
}

/// Construct the semaphore in [`ConnectionManager::new`].
pub(super) fn metadata_limiter() -> Semaphore {
    Semaphore::new(METADATA_PERMITS)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use async_trait::async_trait;
    use mystique_connector_api::{
        ConnectorError, ConnectorResult, ConnectorSession, EngineCapabilities, PoolStatus,
        QueryMode, QueryResult, ValidatedQuery, DEFAULT_TIMEOUT_MS, MAX_TIMEOUT_MS, MIN_TIMEOUT_MS,
        NO_TIMEOUT,
    };

    use super::*;
    use crate::connectors::manager::{ConnectionManager, LiveSession};

    #[tokio::test]
    async fn unknown_execution_id_is_a_no_op() {
        let mgr = ConnectionManager::bootstrap();
        mgr.cancel_query("").await.expect("blank");
        mgr.cancel_query("missing").await.expect("unknown");
    }

    #[tokio::test]
    async fn metadata_permits_stay_bounded() {
        let mgr = Arc::new(ConnectionManager::bootstrap());
        let current = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..20 {
            let mgr = Arc::clone(&mgr);
            let current = Arc::clone(&current);
            let peak = Arc::clone(&peak);
            tasks.spawn(async move {
                mgr.with_metadata(async move {
                    let now = current.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(40)).await;
                    current.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                })
                .await
                .expect("metadata");
            });
        }
        while tasks.join_next().await.is_some() {}
        let high = peak.load(Ordering::SeqCst);
        assert!(high <= METADATA_PERMITS, "peak {high}");
        assert!(high > 1, "permits never overlapped");
    }

    struct TimeoutProbe {
        seen: Arc<std::sync::Mutex<u64>>,
        hang: bool,
    }

    #[async_trait]
    impl ConnectorSession for TimeoutProbe {
        fn engine_id(&self) -> &str {
            "sqlite"
        }

        fn capabilities(&self) -> EngineCapabilities {
            EngineCapabilities::sqlite_defaults()
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
            *self.seen.lock().unwrap_or_else(|err| err.into_inner()) = query.timeout_ms;
            if self.hang {
                query.control.token.cancelled().await;
                return Err(ConnectorError::Query("Query cancelled.".into()));
            }
            Ok(QueryResult {
                engine: "sqlite".into(),
                mode: query.mode,
                columns: Vec::new(),
                rows: Vec::new(),
                row_count: 0,
                truncated: false,
                duration_ms: 0,
                message: None,
            })
        }
    }

    async fn insert_probe(mgr: &ConnectionManager, hang: bool) -> Arc<std::sync::Mutex<u64>> {
        let seen = Arc::new(std::sync::Mutex::new(0));
        let session: Arc<dyn ConnectorSession> = Arc::new(TimeoutProbe {
            seen: Arc::clone(&seen),
            hang,
        });
        mgr.sessions.write().await.insert(
            "probe".into(),
            LiveSession {
                session,
                display_name: "probe".into(),
                opened_at: String::new(),
                product_id: "sqlite".into(),
                _tunnel: None,
                environment: "local".into(),
                read_only: false,
                connection_name: "probe".into(),
                origin: super::super::SessionOrigin::Ui,
                profile_id: None,
                last_used: std::sync::Mutex::new(std::time::Instant::now()),
            },
        );
        seen
    }

    fn request(
        timeout_ms: Option<u64>,
        execution_id: Option<&str>,
    ) -> mystique_connector_api::ExecuteQueryRequest {
        mystique_connector_api::ExecuteQueryRequest {
            session_id: "probe".into(),
            text: "SELECT 1".into(),
            mode: QueryMode::Read,
            max_rows: Some(1),
            timeout_ms,
            confirm_object_name: None,
            collection: None,
            database: None,
            execution_id: execution_id.map(str::to_string),
            params: Vec::new(),
        }
    }

    #[tokio::test]
    async fn timeout_clamp_reaches_the_plugin() {
        let mgr = ConnectionManager::bootstrap();
        let seen = insert_probe(&mgr, false).await;
        mgr.execute_query(request(None, None))
            .await
            .expect("default");
        assert_eq!(
            *seen.lock().unwrap_or_else(|err| err.into_inner()),
            DEFAULT_TIMEOUT_MS
        );
        mgr.execute_query(request(Some(0), None))
            .await
            .expect("zero");
        assert_eq!(
            *seen.lock().unwrap_or_else(|err| err.into_inner()),
            NO_TIMEOUT,
            "0 must reach the plugin as no timeout"
        );
        // A profile may ask for more than the old 120s cap.
        mgr.execute_query(request(Some(600_000), None))
            .await
            .expect("long");
        assert_eq!(*seen.lock().unwrap_or_else(|err| err.into_inner()), 600_000);
        mgr.execute_query(request(Some(u64::MAX), None))
            .await
            .expect("cap");
        assert_eq!(
            *seen.lock().unwrap_or_else(|err| err.into_inner()),
            MAX_TIMEOUT_MS
        );
        mgr.execute_query(request(Some(30_000), None))
            .await
            .expect("agent");
        assert_eq!(*seen.lock().unwrap_or_else(|err| err.into_inner()), 30_000);
        assert_eq!(MIN_TIMEOUT_MS, 1_000);
    }

    #[tokio::test]
    async fn cancel_stops_a_hanging_query() {
        let mgr = ConnectionManager::bootstrap();
        let _seen = insert_probe(&mgr, true).await;
        let mgr = Arc::new(mgr);
        let running = Arc::clone(&mgr);
        let task = tokio::spawn(async move {
            running
                .execute_query(request(Some(120_000), Some("run-1")))
                .await
        });
        tokio::time::sleep(Duration::from_millis(40)).await;
        let started = Instant::now();
        mgr.cancel_query("run-1").await.expect("cancel");
        let result = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("join")
            .expect("task");
        assert!(started.elapsed() < Duration::from_secs(1));
        let err = result.expect_err("cancelled");
        assert!(err.to_string().contains("cancelled"), "{err}");
        mgr.cancel_query("run-1").await.expect("finished id");
    }

    #[tokio::test]
    async fn a_cancel_before_the_run_registers_is_honoured() {
        let mgr = ConnectionManager::bootstrap();
        let seen = insert_probe(&mgr, false).await;
        mgr.cancel_query("run-early").await.expect("early cancel");
        let err = mgr
            .execute_query(request(Some(30_000), Some("run-early")))
            .await
            .expect_err("cancelled before start");
        assert!(err.to_string().contains("cancelled"), "{err}");
        // The plugin never saw the statement.
        assert_eq!(*seen.lock().unwrap_or_else(|err| err.into_inner()), 0);
        assert!(mgr.running.lock().await.is_empty());
        // The early cancel is used once; the next run with a new id runs.
        mgr.execute_query(request(Some(30_000), Some("run-next")))
            .await
            .expect("next run");
    }

    #[tokio::test]
    async fn each_execution_has_its_own_slot() {
        let mgr = ConnectionManager::bootstrap();
        let _seen = insert_probe(&mgr, true).await;
        let mgr = Arc::new(mgr);
        let preview = {
            let mgr = Arc::clone(&mgr);
            tokio::spawn(async move {
                mgr.execute_query(request(Some(120_000), Some("tab-1-preview")))
                    .await
            })
        };
        let export = {
            let mgr = Arc::clone(&mgr);
            tokio::spawn(async move {
                mgr.execute_query(request(Some(120_000), Some("tab-1-export")))
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(40)).await;
        mgr.cancel_query("tab-1-preview").await.expect("cancel");
        let preview = tokio::time::timeout(Duration::from_secs(2), preview)
            .await
            .expect("join")
            .expect("task");
        assert!(preview.is_err());
        assert!(
            !export.is_finished(),
            "export was cancelled with the preview"
        );
        mgr.cancel_query("tab-1-export")
            .await
            .expect("cancel export");
        let _ = tokio::time::timeout(Duration::from_secs(2), export).await;
    }

    #[tokio::test]
    async fn no_timeout_has_no_deadline_but_still_cancels() {
        let mgr = ConnectionManager::bootstrap();
        let _seen = insert_probe(&mgr, true).await;
        let mgr = Arc::new(mgr);
        let running = Arc::clone(&mgr);
        let task = tokio::spawn(async move {
            running
                .execute_query(request(Some(NO_TIMEOUT), Some("run-none")))
                .await
        });
        // Past the 1s floor a clamped 0 used to get.
        tokio::time::sleep(Duration::from_millis(1_300)).await;
        assert!(!task.is_finished(), "a run with no timeout was cut short");
        mgr.cancel_query("run-none").await.expect("cancel");
        let err = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("join")
            .expect("task")
            .expect_err("cancelled");
        assert!(err.to_string().contains("cancelled"), "{err}");
    }

    #[tokio::test]
    async fn deadline_returns_timeout_after_the_plugin_unblocks() {
        let mgr = ConnectionManager::bootstrap();
        let _seen = insert_probe(&mgr, true).await;
        let started = Instant::now();
        let err = mgr
            .execute_query(request(Some(1_000), Some("run-timeout")))
            .await
            .expect_err("timeout");
        assert!(started.elapsed() < Duration::from_secs(3), "hung");
        assert!(matches!(err, ConnectorError::Timeout(_)), "{err}");
        assert!(mgr.running.lock().await.is_empty());
    }
}
