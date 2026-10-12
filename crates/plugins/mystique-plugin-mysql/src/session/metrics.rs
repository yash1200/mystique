//! MySQL / MariaDB server metrics: status, processlist, and statement digest.

use std::collections::HashMap;

use chrono::Utc;
use mysql_async::prelude::*;
use mystique_connector_api::{
    ActiveBackendSession, ConnectorError, ConnectorResult, FetchServerMetricsInput, MetricKind,
    MetricValue, ServerMetricsCapabilities, ServerMetricsSnapshot, ServerOverview, SlowQuery,
    TerminateSessionInput, TerminateSessionResult,
};

use super::query_failed;
use super::MysqlSession;

pub(super) async fn fetch_server_metrics(
    session: &MysqlSession,
    input: &FetchServerMetricsInput,
) -> ConnectorResult<ServerMetricsSnapshot> {
    session.ensure_open()?;
    let mut conn = session.conn().await?;

    let status = show_map(
        &mut conn,
        "SHOW GLOBAL STATUS WHERE Variable_name IN (\
            'Threads_connected','Threads_running','Uptime',\
            'Innodb_buffer_pool_read_requests','Innodb_buffer_pool_reads',\
            'Questions','Slow_queries','Bytes_received','Bytes_sent')",
    )
    .await?;
    let vars = show_map(
        &mut conn,
        "SHOW GLOBAL VARIABLES WHERE Variable_name IN ('max_connections','version')",
    )
    .await?;

    let connected = status_f64(&status, "Threads_connected");
    let max_conn = status_f64(&vars, "max_connections");
    let uptime_ms = status_f64(&status, "Uptime").map(|s| s * 1000.0);
    let running = status_f64(&status, "Threads_running");
    let reqs = status_f64(&status, "Innodb_buffer_pool_read_requests");
    let reads = status_f64(&status, "Innodb_buffer_pool_reads");
    let cache = match (reqs, reads) {
        (Some(r), Some(d)) if r > 0.0 => Some(((r - d) / r).clamp(0.0, 1.0)),
        _ => None,
    };

    let size: Option<u64> = crate::rows::query_first_opt(
        &mut conn,
        "SELECT COALESCE(SUM(data_length + index_length), 0) \
             FROM information_schema.tables \
             WHERE table_schema = DATABASE()",
    )
    .await?;

    let mut extras = Vec::new();
    if let Some(v) = vars.get("version") {
        extras.push(MetricValue::text("version", "Version", v.clone()));
    }
    if let Some(n) = status_f64(&status, "Questions") {
        extras.push(MetricValue::count("questions", "Questions", n));
    }
    if let Some(n) = status_f64(&status, "Slow_queries") {
        extras.push(MetricValue::count("slowQueries", "Slow query counter", n));
    }
    if let Some(n) = status_f64(&status, "Bytes_received") {
        extras.push(MetricValue::bytes("bytesReceived", "Bytes received", n));
    }
    if let Some(n) = status_f64(&status, "Bytes_sent") {
        extras.push(MetricValue::bytes("bytesSent", "Bytes sent", n));
    }

    let overview = ServerOverview {
        connections: match (connected, max_conn) {
            (Some(cur), Some(max)) => MetricValue::count_of("connections", "Connections", cur, max),
            (Some(cur), None) => MetricValue::count("connections", "Connections", cur),
            _ => MetricValue::unavailable(
                "connections",
                "Connections",
                MetricKind::Count,
                "Could not read Threads_connected.",
            ),
        },
        cache_hit_ratio: cache
            .map(|r| MetricValue::ratio_as_percent("cacheHitRatio", "Cache hit", r))
            .unwrap_or_else(|| {
                MetricValue::unavailable(
                    "cacheHitRatio",
                    "Cache hit",
                    MetricKind::Percent,
                    "InnoDB buffer pool stats are not available.",
                )
            }),
        database_size: size
            .map(|n| MetricValue::bytes("databaseSize", "Database size", n as f64))
            .unwrap_or_else(|| {
                MetricValue::unavailable(
                    "databaseSize",
                    "Database size",
                    MetricKind::Bytes,
                    "Could not sum information_schema.tables.",
                )
            }),
        uptime: uptime_ms
            .map(|n| MetricValue::duration_ms("uptime", "Uptime", n))
            .unwrap_or_else(|| {
                MetricValue::unavailable(
                    "uptime",
                    "Uptime",
                    MetricKind::DurationMs,
                    "Could not read Uptime.",
                )
            }),
        active_queries: running
            .map(|n| MetricValue::count("activeQueries", "Active queries", n))
            .unwrap_or_else(|| {
                MetricValue::unavailable(
                    "activeQueries",
                    "Active queries",
                    MetricKind::Count,
                    "Could not read Threads_running.",
                )
            }),
        extras,
    };

    let mut notes = Vec::new();
    let sessions = if input.include_sessions {
        list_sessions(&mut conn).await?
    } else {
        Vec::new()
    };

    let slow_queries = if input.include_slow_queries {
        let mut live = live_slow(&sessions, input.min_duration_ms);
        match list_digests(&mut conn, input.slow_query_limit).await {
            Ok(hist) => {
                if hist.is_empty() {
                    notes.push(
                        "Enable performance_schema statement digest for historical slow queries."
                            .into(),
                    );
                }
                live.extend(hist);
            }
            Err(_) => {
                notes.push(
                    "performance_schema statement digest is not available. Showing live queries only."
                        .into(),
                );
            }
        }
        live.truncate(input.slow_query_limit as usize);
        live
    } else {
        Vec::new()
    };

    Ok(ServerMetricsSnapshot {
        engine: "mysql".into(),
        collected_at: Utc::now().to_rfc3339(),
        capabilities: ServerMetricsCapabilities::full().with_notes(notes),
        overview,
        sessions,
        slow_queries,
    })
}

pub(super) async fn terminate_backend_session(
    session: &MysqlSession,
    input: &TerminateSessionInput,
) -> ConnectorResult<TerminateSessionResult> {
    session.ensure_open()?;
    let id = input.backend_session_id.trim();
    if !id.chars().all(|c| c.is_ascii_digit()) {
        return Err(ConnectorError::InvalidConfig(
            "MySQL session id must be numeric.".into(),
        ));
    }
    let mut conn = session.conn().await?;
    conn.query_drop(format!("KILL CONNECTION {id}"))
        .await
        .map_err(query_failed)?;
    Ok(TerminateSessionResult::ok(id))
}

async fn list_sessions(conn: &mut mysql_async::Conn) -> ConnectorResult<Vec<ActiveBackendSession>> {
    let rows: Vec<(
        u64,
        String,
        Option<String>,
        String,
        Option<i64>,
        Option<String>,
        Option<String>,
    )> = crate::rows::collect_query(
        conn,
        "SELECT ID, USER, DB, COMMAND, TIME, STATE, INFO \
             FROM information_schema.PROCESSLIST \
             ORDER BY TIME DESC, ID",
    )
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, user, db, command, time, state, info)| {
            let mut extras = Vec::new();
            extras.push(MetricValue::text("command", "Command", command.clone()));
            ActiveBackendSession {
                id: id.to_string(),
                user: Some(user).filter(|s| !s.is_empty()),
                database: db.filter(|s| !s.is_empty()),
                state: state.or(Some(command)).filter(|s| !s.is_empty()),
                duration_ms: time.map(|secs| secs.saturating_mul(1000)),
                query: info.filter(|s| !s.trim().is_empty()),
                can_terminate: true,
                extras,
            }
        })
        .collect())
}

fn live_slow(sessions: &[ActiveBackendSession], min_ms: u64) -> Vec<SlowQuery> {
    let min = min_ms as i64;
    sessions
        .iter()
        .filter(|s| {
            s.duration_ms.unwrap_or(0) >= min
                && s.query.as_deref().is_some_and(|q| !q.trim().is_empty())
                && s.state
                    .as_deref()
                    .is_some_and(|st| !st.eq_ignore_ascii_case("sleep"))
        })
        .map(|s| SlowQuery {
            id: format!("live:{}", s.id),
            query: s.query.clone().unwrap_or_default(),
            duration_ms: s.duration_ms,
            mean_ms: None,
            calls: None,
            database: s.database.clone(),
            user: s.user.clone(),
            source: Some("live".into()),
            extras: vec![],
        })
        .collect()
}

async fn list_digests(conn: &mut mysql_async::Conn, limit: u32) -> ConnectorResult<Vec<SlowQuery>> {
    let sql = format!(
        "SELECT DIGEST, DIGEST_TEXT, COUNT_STAR, \
                AVG_TIMER_WAIT / 1000000000.0 AS mean_ms \
         FROM performance_schema.events_statements_summary_by_digest \
         WHERE DIGEST_TEXT IS NOT NULL \
         ORDER BY AVG_TIMER_WAIT DESC \
         LIMIT {limit}"
    );
    let rows: Vec<(Option<String>, Option<String>, u64, Option<f64>)> =
        crate::rows::collect_query(conn, &sql).await?;
    Ok(rows
        .into_iter()
        .enumerate()
        .filter_map(|(i, (digest, text, calls, mean))| {
            let query = text.filter(|s| !s.trim().is_empty())?;
            Some(SlowQuery {
                id: format!("hist:{}", digest.unwrap_or_else(|| i.to_string())),
                query,
                duration_ms: None,
                mean_ms: mean,
                calls: Some(calls as i64),
                database: None,
                user: None,
                source: Some("historical".into()),
                extras: vec![],
            })
        })
        .collect())
}

async fn show_map(
    conn: &mut mysql_async::Conn,
    sql: &str,
) -> ConnectorResult<HashMap<String, String>> {
    let rows: Vec<(String, Option<String>)> = crate::rows::collect_query(conn, sql).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(k, v)| v.map(|value| (k.to_ascii_lowercase(), value)))
        .collect())
}

fn status_f64(map: &HashMap<String, String>, key: &str) -> Option<f64> {
    map.get(&key.to_ascii_lowercase())
        .and_then(|v| v.parse::<f64>().ok())
}
