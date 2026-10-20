//! In-flight statement registry. Cancel looks up `execution_id` and fires the hook.
//!
//! The id is the client token, not the connection profile id. Pinned sessions
//! (a later ticket) must keep using this key so cancel still finds the statement.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use mystique_connector_api::{ConnectorError, ConnectorResult, QueryControl, CANCEL_CLEANUP_MS};
use tokio::sync::Semaphore;
use uuid::Uuid;

use super::ConnectionManager;

pub(super) struct Inflight {
    pub session_id: String,
    control: Arc<QueryControl>,
}

/// Explorer and metrics only. Query execution is not limited by this.
pub(super) const METADATA_PERMITS: usize = 6;

impl ConnectionManager {
    /// Missing and blank ids are a no-op. The button and Esc both land here.
    pub async fn cancel_query(&self, execution_id: &str) -> ConnectorResult<()> {
        let id = execution_id.trim();
        if id.is_empty() {
            return Ok(());
        }
        let control = self
            .running
            .lock()
            .await
            .get(id)
            .map(|run| Arc::clone(&run.control));
        if let Some(control) = control {
            control.cancel();
        }
        Ok(())
    }

    pub(super) async fn begin_run(
        &self,
        session_id: &str,
        execution_id: Option<String>,
    ) -> (String, Arc<QueryControl>) {
        let id = execution_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let control = QueryControl::new();
        self.running.lock().await.insert(
            id.clone(),
            Inflight {
                session_id: session_id.to_string(),
                control: Arc::clone(&control),
            },
        );
        (id, control)
    }

    pub(super) async fn session_busy(&self, session_id: &str) -> bool {
        self.running
            .lock()
            .await
            .values()
            .any(|run| run.session_id == session_id)
    }

    pub(super) async fn cancel_runs_for(&self, session_id: &str) -> bool {
        let controls: Vec<Arc<QueryControl>> = self
            .running
            .lock()
            .await
            .values()
            .filter(|run| run.session_id == session_id)
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
    pub(super) async fn drive_registered<T>(
        &self,
        control: &Arc<QueryControl>,
        timeout_ms: u64,
        work: impl Future<Output = ConnectorResult<T>>,
    ) -> ConnectorResult<T> {
        let cleanup = Duration::from_millis(CANCEL_CLEANUP_MS);
        tokio::pin!(work);
        tokio::select! {
            biased;
            result = &mut work => result,
            _ = control.token.cancelled() => {
                let _ = tokio::time::timeout(cleanup, &mut work).await;
                Err(ConnectorError::Query("Query cancelled.".into()))
            }
            _ = tokio::time::sleep(Duration::from_millis(timeout_ms)) => {
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
        let cleanup = Duration::from_millis(CANCEL_CLEANUP_MS);
        tokio::pin!(work);
        tokio::select! {
            biased;
            result = &mut work => result,
            _ = control.token.cancelled() => {
                let _ = tokio::time::timeout(cleanup, &mut work).await;
                Err(ConnectorError::Query("Query cancelled.".into()))
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
            profile_read_only: false,
            confirm_object_name: None,
            collection: None,
            database: None,
            execution_id: execution_id.map(str::to_string),
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
            DEFAULT_TIMEOUT_MS
        );
        mgr.execute_query(request(Some(9_000_000), None))
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
