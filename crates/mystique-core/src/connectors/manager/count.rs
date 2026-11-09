//! The table grid's background total count.
//!
//! The page never waits for it. The count is registered under the grid's
//! execution id, so the grid can cancel it, and a budget stops it on a large
//! table: past the budget the plugin's COUNT is interrupted and the engine's
//! estimate is returned instead. Under a budget, an unfiltered table whose
//! estimate is already over [`SKIP_EXACT_COUNT_ROWS`] is not counted at all.

use std::time::Duration;

use mystique_connector_api::{
    ConnectorResult, ConnectorSession, FetchTablePreviewInput, FetchTablePreviewStatsRequest,
    TableCountOutcome, TablePreviewCount, TablePreviewStats, CANCEL_CLEANUP_MS,
    SKIP_EXACT_COUNT_ROWS,
};

use super::ConnectionManager;

impl ConnectionManager {
    /// Background COUNT and FK targets. See the module docs.
    pub async fn fetch_table_preview_stats(
        &self,
        req: FetchTablePreviewStatsRequest,
    ) -> ConnectorResult<TablePreviewCount> {
        let budget = req.count_budget_ms.map(Duration::from_millis);
        let run = self.begin_run(&req.session_id, req.execution_id.clone());
        let control = std::sync::Arc::clone(&run.control);
        let session = self.session(&req.session_id).await?;
        let mut input = req.into_input();
        input.control = std::sync::Arc::clone(&control);
        let unfiltered = input.filters.is_empty() && input.code_filter.is_none();

        let mut estimate = None;
        let mut skipped = false;
        if budget.is_some() && unfiltered && !control.is_cancelled() {
            estimate = estimate_rows(session.as_ref(), &input).await;
            if estimate.is_some_and(|rows| rows >= SKIP_EXACT_COUNT_ROWS) {
                // Plugins check the flag before they COUNT; FK targets still load.
                skipped = true;
                control.cancel();
            }
        }

        let work = session.fetch_table_preview_stats(&input);
        tokio::pin!(work);
        let grace = Duration::from_millis(CANCEL_CLEANUP_MS);
        let mut over_budget = false;
        let stats = if skipped {
            tokio::time::timeout(grace, &mut work)
                .await
                .unwrap_or_else(|_| Ok(TablePreviewStats::default()))
        } else {
            let deadline = async {
                match budget {
                    Some(budget) => tokio::time::sleep(budget).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                biased;
                result = &mut work => result,
                _ = control.token.cancelled() => {
                    tokio::time::timeout(grace, &mut work)
                        .await
                        .unwrap_or_else(|_| Ok(TablePreviewStats::default()))
                }
                _ = deadline => {
                    over_budget = true;
                    control.cancel();
                    tokio::time::timeout(grace, &mut work)
                        .await
                        .unwrap_or_else(|_| Ok(TablePreviewStats::default()))
                }
            }
        };
        // A plugin that fails the whole call (not only the COUNT) leaves the
        // total unknown, as before.
        let stats = stats.unwrap_or_default();
        let outcome = if stats.total_known {
            TableCountOutcome::Exact
        } else if skipped {
            TableCountOutcome::Skipped
        } else if over_budget {
            TableCountOutcome::OverBudget
        } else if control.is_cancelled() {
            TableCountOutcome::Cancelled
        } else {
            TableCountOutcome::Failed
        };
        if !stats.total_known && estimate.is_none() {
            estimate = estimate_rows(session.as_ref(), &input).await;
        }
        Ok(TablePreviewCount {
            total_rows: stats.total_rows,
            total_known: stats.total_known,
            references: stats.references,
            outcome,
            estimated_rows: if stats.total_known { None } else { estimate },
        })
    }
}

/// An estimate is a planner call; a lock or a slow server must not hold the
/// count's answer for long.
const ESTIMATE_TIMEOUT: Duration = Duration::from_secs(2);

async fn estimate_rows(
    session: &dyn ConnectorSession,
    input: &FetchTablePreviewInput,
) -> Option<u64> {
    tokio::time::timeout(ESTIMATE_TIMEOUT, session.estimate_table_rows(input))
        .await
        .ok()?
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Instant;

    use async_trait::async_trait;
    use mystique_connector_api::{
        ConnectorError, EngineCapabilities, PoolStatus, QueryResult, TableFilter, ValidatedQuery,
    };

    use super::super::{LiveSession, SessionOrigin};
    use super::*;

    /// COUNT takes `count_ms` (or until cancelled). The estimate is fixed.
    struct CountProbe {
        count_ms: u64,
        estimate: Option<u64>,
        counted: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ConnectorSession for CountProbe {
        fn engine_id(&self) -> &str {
            "postgres"
        }

        fn capabilities(&self) -> EngineCapabilities {
            EngineCapabilities::postgres_defaults()
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

        async fn execute_query(&self, _query: &ValidatedQuery) -> ConnectorResult<QueryResult> {
            Err(ConnectorError::NotSupported("probe".into()))
        }

        async fn fetch_table_preview_stats(
            &self,
            input: &FetchTablePreviewInput,
        ) -> ConnectorResult<TablePreviewStats> {
            let count = async {
                if input.control.is_cancelled() {
                    return None;
                }
                self.counted.fetch_add(1, Ordering::SeqCst);
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(self.count_ms)) => Some(7),
                    _ = input.control.token.cancelled() => None,
                }
            };
            let total = count.await;
            Ok(TablePreviewStats {
                total_rows: total.unwrap_or(0),
                total_known: total.is_some(),
                references: Vec::new(),
            })
        }

        async fn estimate_table_rows(
            &self,
            _input: &FetchTablePreviewInput,
        ) -> ConnectorResult<Option<u64>> {
            Ok(self.estimate)
        }
    }

    async fn manager(
        count_ms: u64,
        estimate: Option<u64>,
    ) -> (Arc<ConnectionManager>, Arc<AtomicUsize>) {
        let mgr = ConnectionManager::bootstrap();
        let counted = Arc::new(AtomicUsize::new(0));
        let session: Arc<dyn ConnectorSession> = Arc::new(CountProbe {
            count_ms,
            estimate,
            counted: Arc::clone(&counted),
        });
        mgr.sessions.write().await.insert(
            "probe".into(),
            LiveSession {
                session,
                display_name: "probe".into(),
                opened_at: String::new(),
                product_id: "postgres".into(),
                _tunnel: None,
                environment: "local".into(),
                read_only: false,
                connection_name: "probe".into(),
                origin: SessionOrigin::Ui,
                profile_id: None,
                last_used: std::sync::Mutex::new(Instant::now()),
            },
        );
        (Arc::new(mgr), counted)
    }

    fn request(budget_ms: Option<u64>, id: &str) -> FetchTablePreviewStatsRequest {
        FetchTablePreviewStatsRequest {
            session_id: "probe".into(),
            table: "t".into(),
            schema: None,
            database: None,
            filters: Vec::new(),
            code_filter: None,
            execution_id: Some(id.into()),
            count_budget_ms: budget_ms,
        }
    }

    #[tokio::test]
    async fn a_fast_count_is_exact() {
        let (mgr, _) = manager(5, Some(9)).await;
        let count = mgr
            .fetch_table_preview_stats(request(Some(1_000), "c1"))
            .await
            .expect("count");
        assert_eq!(count.outcome, TableCountOutcome::Exact);
        assert!(count.total_known);
        assert_eq!(count.total_rows, 7);
        assert_eq!(count.estimated_rows, None);
    }

    #[tokio::test]
    async fn a_slow_count_stops_at_the_budget_with_the_estimate() {
        let (mgr, _) = manager(60_000, Some(1_234)).await;
        let started = Instant::now();
        let count = mgr
            .fetch_table_preview_stats(request(Some(50), "c2"))
            .await
            .expect("count");
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(count.outcome, TableCountOutcome::OverBudget);
        assert!(!count.total_known);
        assert_eq!(count.estimated_rows, Some(1_234));
    }

    #[tokio::test]
    async fn a_huge_table_is_not_counted_under_a_budget() {
        let (mgr, counted) = manager(60_000, Some(SKIP_EXACT_COUNT_ROWS)).await;
        let count = mgr
            .fetch_table_preview_stats(request(Some(5_000), "c3"))
            .await
            .expect("count");
        assert_eq!(count.outcome, TableCountOutcome::Skipped);
        assert_eq!(count.estimated_rows, Some(SKIP_EXACT_COUNT_ROWS));
        assert_eq!(counted.load(Ordering::SeqCst), 0);

        // An explicit count (no budget) runs anyway.
        let (mgr, counted) = manager(5, Some(SKIP_EXACT_COUNT_ROWS)).await;
        let count = mgr
            .fetch_table_preview_stats(request(None, "c4"))
            .await
            .expect("count");
        assert_eq!(count.outcome, TableCountOutcome::Exact);
        assert_eq!(counted.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_filtered_count_is_not_skipped() {
        let (mgr, counted) = manager(5, Some(SKIP_EXACT_COUNT_ROWS)).await;
        let mut req = request(Some(5_000), "c5");
        req.filters = vec![TableFilter {
            column: "a".into(),
            op: "eq".into(),
            value: "1".into(),
        }];
        let count = mgr.fetch_table_preview_stats(req).await.expect("count");
        assert_eq!(count.outcome, TableCountOutcome::Exact);
        assert_eq!(counted.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn the_grid_can_cancel_the_count() {
        let (mgr, _) = manager(60_000, Some(55)).await;
        let running = Arc::clone(&mgr);
        let task =
            tokio::spawn(
                async move { running.fetch_table_preview_stats(request(None, "c6")).await },
            );
        tokio::time::sleep(Duration::from_millis(40)).await;
        mgr.cancel_query("c6").await.expect("cancel");
        let count = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("join")
            .expect("task")
            .expect("count");
        assert_eq!(count.outcome, TableCountOutcome::Cancelled);
        assert_eq!(count.estimated_rows, Some(55));
    }
}
