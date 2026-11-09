//! SQLite file metrics. No server process list or historical slow log.

use chrono::Utc;
use mystique_connector_api::{
    ActiveBackendSession, ConnectorError, ConnectorResult, FetchServerMetricsInput, MetricKind,
    MetricValue, ServerMetricsCapabilities, ServerMetricsSnapshot, ServerOverview,
    TerminateSessionInput, TerminateSessionResult,
};

use super::query_failed;
use super::SqliteSession;

pub(super) async fn fetch_server_metrics(
    session: &SqliteSession,
    input: &FetchServerMetricsInput,
) -> ConnectorResult<ServerMetricsSnapshot> {
    session.ensure_open()?;
    let pool = session.pool();
    let include_sessions = input.include_sessions;
    session
        .blocking(move || collect(&pool, include_sessions))
        .await
}

pub(super) async fn terminate_backend_session(
    _session: &SqliteSession,
    input: &TerminateSessionInput,
) -> ConnectorResult<TerminateSessionResult> {
    Err(ConnectorError::NotSupported(format!(
        "SQLite has no server sessions to terminate (id `{}`).",
        input.backend_session_id
    )))
}

fn collect(
    pool: &r2d2::Pool<crate::connector::manager::SqliteConnectionManager>,
    include_sessions: bool,
) -> ConnectorResult<ServerMetricsSnapshot> {
    let conn = pool
        .get()
        .map_err(|e| ConnectorError::Pool(e.to_string()))?;

    let page_count: i64 = conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .map_err(query_failed)?;
    let page_size: i64 = conn
        .query_row("PRAGMA page_size", [], |r| r.get(0))
        .map_err(query_failed)?;
    let cache_size: i64 = conn
        .query_row("PRAGMA cache_size", [], |r| r.get(0))
        .unwrap_or(0);
    let journal: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap_or_else(|_| "unknown".into());
    let fk: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap_or(0);

    let size = (page_count.saturating_mul(page_size)) as f64;
    let extras = vec![
        MetricValue::text("journalMode", "Journal mode", journal),
        MetricValue::bytes("pageSize", "Page size", page_size as f64),
        MetricValue::count("cacheSize", "Cache size (pages)", cache_size as f64),
        MetricValue::text(
            "foreignKeys",
            "Foreign keys",
            if fk == 0 { "off" } else { "on" },
        ),
    ];

    let sessions = if include_sessions {
        vec![ActiveBackendSession {
            id: "local".into(),
            user: None,
            database: Some("main".into()),
            state: Some("idle".into()),
            duration_ms: None,
            query: None,
            can_terminate: false,
            extras: vec![MetricValue::text(
                "note",
                "Note",
                "This process is the only client of the file.",
            )],
        }]
    } else {
        Vec::new()
    };

    Ok(ServerMetricsSnapshot {
        engine: "sqlite".into(),
        collected_at: Utc::now().to_rfc3339(),
        capabilities: ServerMetricsCapabilities {
            overview: true,
            active_sessions: true,
            terminate_session: false,
            slow_queries: false,
            notes: vec![
                "SQLite is a file database. There is no server cache-hit ratio, uptime, or historical slow-query log.".into(),
            ],
        },
        overview: ServerOverview {
            connections: MetricValue::count("connections", "Connections", 1.0),
            cache_hit_ratio: MetricValue::unavailable(
                "cacheHitRatio",
                "Cache hit",
                MetricKind::Percent,
                "SQLite does not expose a cache hit ratio over SQL.",
            ),
            database_size: MetricValue::bytes("databaseSize", "Database size", size),
            uptime: MetricValue::unavailable(
                "uptime",
                "Uptime",
                MetricKind::DurationMs,
                "SQLite has no server process uptime.",
            ),
            active_queries: MetricValue::count("activeQueries", "Active queries", 0.0),
            extras,
        },
        sessions,
        slow_queries: Vec::new(),
    })
}
