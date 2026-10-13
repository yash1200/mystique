//! PostgreSQL server metrics: activity, cache, size, and optional statements.

use chrono::Utc;
use mystique_connector_api::{
    ActiveBackendSession, ConnectorResult, FetchServerMetricsInput, MetricKind, MetricValue,
    ServerMetricsCapabilities, ServerMetricsSnapshot, ServerOverview, SlowQuery,
    TerminateSessionInput, TerminateSessionResult,
};
use tokio_postgres::Row;

use super::query_failed;
use super::PostgresSession;

/// Activity stats that exist on both Postgres and Cockroach.
/// Size and uptime stay in their own statements: Cockroach rejects
/// `pg_postmaster_start_time`, and a missing `pg_database_size` used to
/// fail this whole snapshot.
const ACTIVITY_SQL: &str = r#"
SELECT
  (SELECT count(*)::bigint FROM pg_stat_activity) AS connections,
  (
    SELECT setting::double precision
    FROM pg_settings
    WHERE name = 'max_connections'
  ) AS max_connections,
  (
    SELECT CASE
      WHEN coalesce(sum(blks_hit + blks_read), 0) = 0 THEN NULL
      ELSE sum(blks_hit)::double precision
           / sum(blks_hit + blks_read)::double precision
    END
    FROM pg_stat_database
    WHERE datname = current_database()
  ) AS cache_hit,
  (
    SELECT count(*)::bigint
    FROM pg_stat_activity
    WHERE state = 'active' AND pid <> pg_backend_pid()
  ) AS active_queries
"#;

const SIZE_SQL: &str = "SELECT pg_database_size(current_database())";

const UPTIME_SQL: &str = r#"
SELECT (extract(epoch FROM now() - pg_postmaster_start_time()) * 1000)::double precision
"#;

pub(super) async fn fetch_server_metrics(
    session: &PostgresSession,
    input: &FetchServerMetricsInput,
) -> ConnectorResult<ServerMetricsSnapshot> {
    session.ensure_open()?;
    let client = session.client().await?;

    let own_pid: i32 = client
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .map_err(query_failed)?
        .try_get(0)
        .unwrap_or(0);

    let overview_row = client
        .query_one(ACTIVITY_SQL, &[])
        .await
        .map_err(query_failed)?;

    let connections = f64_cell(&overview_row, 0);
    let max_conn = f64_cell(&overview_row, 1);
    let cache = f64_cell(&overview_row, 2);
    let active = f64_cell(&overview_row, 3);
    let db_size = optional_metric(&client, SIZE_SQL).await;
    let uptime_ms = optional_metric(&client, UPTIME_SQL).await;

    let mut extras = Vec::new();
    if let Ok(ver) = client.query_one("SHOW server_version", &[]).await {
        if let Ok(v) = ver.try_get::<_, String>(0) {
            extras.push(MetricValue::text("version", "Version", v));
        }
    }
    if let Ok(row) = client
        .query_opt(
            r#"
            SELECT xact_commit, xact_rollback, temp_bytes, deadlocks
            FROM pg_stat_database
            WHERE datname = current_database()
            "#,
            &[],
        )
        .await
    {
        if let Some(row) = row {
            if let Some(n) = f64_cell(&row, 0) {
                extras.push(MetricValue::count(
                    "xactCommit",
                    "Transactions committed",
                    n,
                ));
            }
            if let Some(n) = f64_cell(&row, 1) {
                extras.push(MetricValue::count(
                    "xactRollback",
                    "Transactions rolled back",
                    n,
                ));
            }
            if let Some(n) = f64_cell(&row, 2) {
                extras.push(MetricValue::bytes("tempBytes", "Temp bytes", n));
            }
            if let Some(n) = f64_cell(&row, 3) {
                extras.push(MetricValue::count("deadlocks", "Deadlocks", n));
            }
        }
    }

    let overview = ServerOverview {
        connections: match (connections, max_conn) {
            (Some(cur), Some(max)) => MetricValue::count_of("connections", "Connections", cur, max),
            (Some(cur), None) => MetricValue::count("connections", "Connections", cur),
            _ => MetricValue::unavailable(
                "connections",
                "Connections",
                MetricKind::Count,
                "Could not read pg_stat_activity.",
            ),
        },
        cache_hit_ratio: cache
            .map(|r| MetricValue::ratio_as_percent("cacheHitRatio", "Cache hit", r))
            .unwrap_or_else(|| {
                MetricValue::unavailable(
                    "cacheHitRatio",
                    "Cache hit",
                    MetricKind::Percent,
                    "No block I/O recorded for this database yet.",
                )
            }),
        database_size: db_size
            .map(|n| MetricValue::bytes("databaseSize", "Database size", n))
            .unwrap_or_else(|| {
                MetricValue::unavailable(
                    "databaseSize",
                    "Database size",
                    MetricKind::Bytes,
                    "Could not read pg_database_size.",
                )
            }),
        uptime: uptime_ms
            .map(|n| MetricValue::duration_ms("uptime", "Uptime", n))
            .unwrap_or_else(|| {
                MetricValue::unavailable(
                    "uptime",
                    "Uptime",
                    MetricKind::DurationMs,
                    "Could not read postmaster start time.",
                )
            }),
        active_queries: active
            .map(|n| MetricValue::count("activeQueries", "Active queries", n))
            .unwrap_or_else(|| {
                MetricValue::unavailable(
                    "activeQueries",
                    "Active queries",
                    MetricKind::Count,
                    "Could not count active backends.",
                )
            }),
        extras,
    };

    let mut notes = Vec::new();
    let sessions = if input.include_sessions {
        match list_sessions(&client, own_pid).await {
            Ok(rows) => rows,
            Err(err) => {
                notes.push(format!("Sessions unavailable: {err}"));
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    let slow_queries = if input.include_slow_queries {
        let mut live = match list_live_slow(&client, own_pid, input.min_duration_ms).await {
            Ok(rows) => rows,
            Err(err) => {
                notes.push(format!("Live queries unavailable: {err}"));
                Vec::new()
            }
        };
        match list_statement_stats(&client, input.slow_query_limit).await {
            Ok(hist) => {
                if hist.is_empty() {
                    notes.push("Install pg_stat_statements to see historical mean times.".into());
                }
                live.extend(hist);
            }
            Err(_) => {
                notes
                    .push("pg_stat_statements is not available. Showing live queries only.".into());
            }
        }
        live.truncate(input.slow_query_limit as usize);
        live
    } else {
        Vec::new()
    };

    Ok(ServerMetricsSnapshot {
        engine: "postgres".into(),
        collected_at: Utc::now().to_rfc3339(),
        capabilities: ServerMetricsCapabilities::full().with_notes(notes),
        overview,
        sessions,
        slow_queries,
    })
}

pub(super) async fn terminate_backend_session(
    session: &PostgresSession,
    input: &TerminateSessionInput,
) -> ConnectorResult<TerminateSessionResult> {
    session.ensure_open()?;
    let id = input.backend_session_id.trim();
    let pid: i32 = id.parse().map_err(|_| {
        mystique_connector_api::ConnectorError::InvalidConfig(
            "Postgres session id must be a pid.".into(),
        )
    })?;
    let client = session.client().await?;
    let own_pid: i32 = client
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .map_err(query_failed)?
        .try_get(0)
        .unwrap_or(0);
    if pid == own_pid {
        return Ok(TerminateSessionResult::failed(
            id,
            "Cannot terminate Mystique’s own backend.",
        ));
    }
    let ok: bool = client
        .query_one("SELECT pg_terminate_backend($1)", &[&pid])
        .await
        .map_err(query_failed)?
        .try_get(0)
        .unwrap_or(false);
    if ok {
        Ok(TerminateSessionResult::ok(id))
    } else {
        Ok(TerminateSessionResult::failed(
            id,
            "Server refused to terminate this backend.",
        ))
    }
}

async fn list_sessions(
    client: &deadpool_postgres::Object,
    own_pid: i32,
) -> ConnectorResult<Vec<ActiveBackendSession>> {
    let rows = client
        .query(
            r#"
            SELECT
              pid,
              usename::text,
              datname::text,
              state,
              (extract(epoch FROM (now() - coalesce(query_start, xact_start, backend_start)))
                * 1000)::double precision AS duration_ms,
              query,
              application_name,
              client_addr::text,
              wait_event
            FROM pg_stat_activity
            ORDER BY query_start NULLS LAST, pid
            "#,
            &[],
        )
        .await
        .map_err(query_failed)?;

    Ok(rows
        .iter()
        .filter_map(|row| {
            let pid: i32 = row.try_get(0).ok()?;
            let mut extras = Vec::new();
            if let Some(app) = opt_string(row, 6) {
                extras.push(MetricValue::text("application", "Application", app));
            }
            if let Some(addr) = opt_string(row, 7) {
                extras.push(MetricValue::text("client", "Client", addr));
            }
            if let Some(wait) = opt_string(row, 8) {
                extras.push(MetricValue::text("waitEvent", "Wait", wait));
            }
            Some(ActiveBackendSession {
                id: pid.to_string(),
                user: opt_string(row, 1),
                database: opt_string(row, 2),
                state: opt_string(row, 3),
                duration_ms: f64_cell(row, 4).map(|n| n.round() as i64),
                query: opt_string(row, 5).filter(|q| !q.trim().is_empty()),
                can_terminate: pid != own_pid,
                extras,
            })
        })
        .collect())
}

async fn list_live_slow(
    client: &deadpool_postgres::Object,
    own_pid: i32,
    min_ms: u64,
) -> ConnectorResult<Vec<SlowQuery>> {
    let min = min_ms as f64;
    let rows = client
        .query(
            r#"
            SELECT
              pid,
              query,
              (extract(epoch FROM (now() - query_start)) * 1000)::double precision AS duration_ms,
              datname::text,
              usename::text
            FROM pg_stat_activity
            WHERE state = 'active'
              AND pid <> $1
              AND query_start IS NOT NULL
              AND (extract(epoch FROM (now() - query_start)) * 1000)::double precision >= $2
            ORDER BY query_start
            "#,
            &[&own_pid, &min],
        )
        .await
        .map_err(query_failed)?;

    Ok(rows
        .iter()
        .filter_map(|row| {
            let pid: i32 = row.try_get(0).ok()?;
            Some(SlowQuery {
                id: format!("live:{pid}"),
                query: opt_string(row, 1).unwrap_or_default(),
                duration_ms: f64_cell(row, 2).map(|n| n.round() as i64),
                mean_ms: None,
                calls: None,
                database: opt_string(row, 3),
                user: opt_string(row, 4),
                source: Some("live".into()),
                extras: vec![],
            })
        })
        .collect())
}

async fn list_statement_stats(
    client: &deadpool_postgres::Object,
    limit: u32,
) -> ConnectorResult<Vec<SlowQuery>> {
    let enabled: bool = client
        .query_one(
            "SELECT exists(SELECT 1 FROM pg_extension WHERE extname = 'pg_stat_statements')",
            &[],
        )
        .await
        .map_err(query_failed)?
        .try_get(0)
        .unwrap_or(false);
    if !enabled {
        return Ok(Vec::new());
    }

    let limit_i = i64::from(limit);
    let modern = r#"
        SELECT
          s.queryid::text,
          s.query,
          s.mean_exec_time::double precision,
          s.calls,
          d.datname::text
        FROM pg_stat_statements s
        LEFT JOIN pg_database d ON d.oid = s.dbid
        WHERE s.query IS NOT NULL AND btrim(s.query) <> ''
        ORDER BY s.mean_exec_time DESC
        LIMIT $1
    "#;
    let legacy = r#"
        SELECT
          s.queryid::text,
          s.query,
          s.mean_time::double precision,
          s.calls,
          d.datname::text
        FROM pg_stat_statements s
        LEFT JOIN pg_database d ON d.oid = s.dbid
        WHERE s.query IS NOT NULL AND btrim(s.query) <> ''
        ORDER BY s.mean_time DESC
        LIMIT $1
    "#;

    let rows = match client.query(modern, &[&limit_i]).await {
        Ok(rows) => rows,
        Err(_) => client
            .query(legacy, &[&limit_i])
            .await
            .map_err(query_failed)?,
    };

    Ok(rows
        .iter()
        .enumerate()
        .filter_map(|(i, row)| {
            let id = opt_string(row, 0).unwrap_or_else(|| format!("hist:{i}"));
            Some(SlowQuery {
                id: format!("hist:{id}"),
                query: opt_string(row, 1).unwrap_or_default(),
                duration_ms: None,
                mean_ms: f64_cell(row, 2),
                calls: f64_cell(row, 3).map(|n| n.round() as i64),
                database: opt_string(row, 4),
                user: None,
                source: Some("historical".into()),
                extras: vec![],
            })
        })
        .collect())
}

fn f64_cell(row: &Row, idx: usize) -> Option<f64> {
    super::encode::metric_f64(row, idx)
}

async fn optional_metric(client: &deadpool_postgres::Object, sql: &str) -> Option<f64> {
    let row = client.query_one(sql, &[]).await.ok()?;
    f64_cell(&row, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_and_uptime_are_separate_from_activity() {
        assert!(!ACTIVITY_SQL.contains("pg_database_size"));
        assert!(!ACTIVITY_SQL.contains("pg_postmaster_start_time"));
        assert!(SIZE_SQL.contains("pg_database_size"));
        assert!(UPTIME_SQL.contains("pg_postmaster_start_time"));
    }
}

fn opt_string(row: &Row, idx: usize) -> Option<String> {
    row.try_get::<_, Option<String>>(idx)
        .ok()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
