//! MongoDB server metrics: serverStatus, currentOp, dbStats, killOp.
//!
//! Cluster commands (`serverStatus`, `currentOp` for all users) need
//! `clusterMonitor`. Atlas / app users usually do not have that. We try
//! those first, then fall back to the selected database (`dbStats`,
//! `buildInfo`, own operations) and never fail the whole tab.

use chrono::Utc;
use mongodb::bson::{doc, Bson, Document};
use mystique_connector_api::{
    ActiveBackendSession, ConnectorResult, FetchServerMetricsInput, MetricKind, MetricValue,
    ServerMetricsCapabilities, ServerMetricsSnapshot, ServerOverview, SlowQuery,
    TerminateSessionInput, TerminateSessionResult,
};

use super::error::is_unauthorized;
use super::query_failed;
use super::MongoSession;

const PRIVILEGE_NOTE: &str = "This user cannot run cluster commands on admin (needs clusterMonitor). Showing database-level stats instead.";

pub(super) async fn fetch_server_metrics(
    session: &MongoSession,
    input: &FetchServerMetricsInput,
) -> ConnectorResult<ServerMetricsSnapshot> {
    session.ensure_open()?;
    let mut notes = Vec::new();
    let db_name = session.resolve_database(None).ok();

    let status = match load_server_status(session, db_name.as_deref()).await {
        Ok(Some(doc)) => Some(doc),
        Ok(None) => {
            notes.push(PRIVILEGE_NOTE.into());
            None
        }
        Err(e) => {
            notes.push(e.to_string());
            None
        }
    };

    let connections = status
        .as_ref()
        .and_then(|s| s.get_document("connections").ok());
    let current = connections.and_then(|c| bson_f64(c.get("current")));
    let available = connections.and_then(|c| bson_f64(c.get("available")));
    let max = match (current, available) {
        (Some(c), Some(a)) => Some(c + a),
        _ => None,
    };
    let active_conn = connections.and_then(|c| bson_f64(c.get("active")));

    let uptime_ms = status
        .as_ref()
        .and_then(|s| bson_f64(s.get("uptime")))
        .map(|s| s * 1000.0);
    let (cache, cache_bytes) = status
        .as_ref()
        .map(wired_tiger_cache)
        .unwrap_or((None, None));

    let mut extras = Vec::new();
    if let Some(ver) = status
        .as_ref()
        .and_then(|s| s.get_str("version").ok())
        .map(str::to_string)
    {
        extras.push(MetricValue::text("version", "Version", ver));
    } else if let Some(ver) = load_build_info_version(session, db_name.as_deref()).await {
        extras.push(MetricValue::text("version", "Version", ver));
    }
    if let Some(n) = cache_bytes {
        extras.push(MetricValue::bytes("wiredTigerCache", "WiredTiger cache", n));
    }
    if let Some(mem) = status.as_ref().and_then(|s| s.get_document("mem").ok()) {
        if let Some(n) = bson_f64(mem.get("resident")) {
            extras.push(MetricValue::bytes(
                "residentMb",
                "Resident memory",
                n * 1024.0 * 1024.0,
            ));
        }
    }
    if let Some(ops) = status
        .as_ref()
        .and_then(|s| s.get_document("opcounters").ok())
    {
        if let Some(n) = bson_f64(ops.get("query")) {
            extras.push(MetricValue::count("opQuery", "Queries", n));
        }
        if let Some(n) = bson_f64(ops.get("insert")) {
            extras.push(MetricValue::count("opInsert", "Inserts", n));
        }
        if let Some(n) = bson_f64(ops.get("update")) {
            extras.push(MetricValue::count("opUpdate", "Updates", n));
        }
    }

    let mut size = None;
    if let Some(db_name) = db_name.as_deref() {
        if let Ok(stats) = session
            .client
            .database(db_name)
            .run_command(doc! { "dbStats": 1 })
            .await
        {
            size = bson_f64(stats.get("dataSize"));
            if let Some(n) = bson_f64(stats.get("indexSize")) {
                extras.push(MetricValue::bytes("indexSize", "Index size", n));
            }
            if let Some(n) = bson_f64(stats.get("objects")) {
                extras.push(MetricValue::count("objects", "Documents", n));
            }
        }
    }

    let missing_cluster = status.is_none();
    let overview = ServerOverview {
        connections: match (current, max) {
            (Some(cur), Some(m)) => MetricValue::count_of("connections", "Connections", cur, m),
            (Some(cur), None) => MetricValue::count("connections", "Connections", cur),
            _ => MetricValue::unavailable(
                "connections",
                "Connections",
                MetricKind::Count,
                if missing_cluster {
                    "Needs clusterMonitor to read serverStatus."
                } else {
                    "serverStatus.connections is missing."
                },
            ),
        },
        cache_hit_ratio: cache
            .map(|r| MetricValue::ratio_as_percent("cacheHitRatio", "Cache hit", r))
            .unwrap_or_else(|| {
                MetricValue::unavailable(
                    "cacheHitRatio",
                    "Cache hit",
                    MetricKind::Percent,
                    if missing_cluster {
                        "Needs clusterMonitor to read WiredTiger cache stats."
                    } else {
                        "WiredTiger cache stats are not available (in-memory / non-WT storage)."
                    },
                )
            }),
        database_size: size
            .map(|n| MetricValue::bytes("databaseSize", "Database size", n))
            .unwrap_or_else(|| {
                MetricValue::unavailable(
                    "databaseSize",
                    "Database size",
                    MetricKind::Bytes,
                    "Select a database or add `/dbname` to the URI.",
                )
            }),
        uptime: uptime_ms
            .map(|n| MetricValue::duration_ms("uptime", "Uptime", n))
            .unwrap_or_else(|| {
                MetricValue::unavailable(
                    "uptime",
                    "Uptime",
                    MetricKind::DurationMs,
                    if missing_cluster {
                        "Needs clusterMonitor to read server uptime."
                    } else {
                        "serverStatus.uptime is missing."
                    },
                )
            }),
        active_queries: active_conn
            .map(|n| MetricValue::count("activeQueries", "Active queries", n))
            .unwrap_or_else(|| {
                MetricValue::unavailable(
                    "activeQueries",
                    "Active queries",
                    MetricKind::Count,
                    if missing_cluster {
                        "Needs clusterMonitor to read active connections."
                    } else {
                        "serverStatus.connections.active is missing."
                    },
                )
            }),
        extras,
    };

    let (sessions, slow_live, own_ops_only) =
        if input.include_sessions || input.include_slow_queries {
            list_ops(session, db_name.as_deref(), input.min_duration_ms).await
        } else {
            (Vec::new(), Vec::new(), false)
        };
    if own_ops_only {
        notes.push(
            "Showing this user’s operations only. Grant inprog / clusterMonitor to see every session."
                .into(),
        );
    }
    let sessions = if input.include_sessions {
        sessions
    } else {
        Vec::new()
    };
    let slow_queries = if input.include_slow_queries {
        let mut live = slow_live;
        match list_profile(session, input.slow_query_limit).await {
            Ok(hist) => {
                if hist.is_empty() {
                    notes.push(
                        "Enable the database profiler (level 1 or 2) for historical slow queries."
                            .into(),
                    );
                }
                live.extend(hist);
            }
            Err(_) => {
                notes.push(
                    "system.profile is not readable. Showing current operations only.".into(),
                );
            }
        }
        live.truncate(input.slow_query_limit as usize);
        live
    } else {
        Vec::new()
    };

    Ok(ServerMetricsSnapshot {
        engine: "mongodb".into(),
        collected_at: Utc::now().to_rfc3339(),
        capabilities: ServerMetricsCapabilities::full().with_notes(notes),
        overview,
        sessions,
        slow_queries,
    })
}

async fn load_server_status(
    session: &MongoSession,
    db_name: Option<&str>,
) -> ConnectorResult<Option<Document>> {
    let mut names: Vec<String> = vec!["admin".into()];
    if let Some(db) = db_name {
        if db != "admin" {
            names.push(db.to_string());
        }
    }
    let mut last_err = None;
    for name in names {
        match session
            .client
            .database(&name)
            .run_command(doc! { "serverStatus": 1 })
            .await
        {
            Ok(doc) => return Ok(Some(doc)),
            Err(e) if is_unauthorized(&e) => {
                last_err = Some(e);
            }
            Err(e) => return Err(query_failed(e)),
        }
    }
    let _ = last_err;
    Ok(None)
}

async fn load_build_info_version(session: &MongoSession, db_name: Option<&str>) -> Option<String> {
    let name = db_name.unwrap_or("admin");
    let doc = session
        .client
        .database(name)
        .run_command(doc! { "buildInfo": 1 })
        .await
        .ok()?;
    doc.get_str("version").ok().map(str::to_string)
}

pub(super) async fn terminate_backend_session(
    session: &MongoSession,
    input: &TerminateSessionInput,
) -> ConnectorResult<TerminateSessionResult> {
    session.ensure_open()?;
    let id = input.backend_session_id.trim();
    let op = parse_opid(id);
    let result = match session
        .client
        .database("admin")
        .run_command(doc! { "killOp": 1, "op": op })
        .await
    {
        Ok(doc) => doc,
        Err(e) if is_unauthorized(&e) => {
            return Ok(TerminateSessionResult::failed(
                id,
                "Not authorized to kill operations. Grant killop / clusterMonitor.",
            ));
        }
        Err(e) => return Err(query_failed(e)),
    };
    let info = result
        .get_str("info")
        .ok()
        .map(str::to_string)
        .or_else(|| result.get_str("errmsg").ok().map(str::to_string));
    let ok = result.get_f64("ok").ok().unwrap_or(0.0) >= 1.0
        || result.get_i32("ok").ok().unwrap_or(0) >= 1;
    if ok {
        Ok(TerminateSessionResult {
            ok: true,
            backend_session_id: id.to_string(),
            message: info,
        })
    } else {
        Ok(TerminateSessionResult::failed(
            id,
            info.unwrap_or_else(|| "killOp failed.".into()),
        ))
    }
}

async fn list_ops(
    session: &MongoSession,
    db_name: Option<&str>,
    min_ms: u64,
) -> (Vec<ActiveBackendSession>, Vec<SlowQuery>, bool) {
    let doc = match load_current_op(session, db_name).await {
        Some(pair) => pair,
        None => return (Vec::new(), Vec::new(), false),
    };
    let (doc, own_ops_only) = doc;
    let inprog = match doc.get_array("inprog") {
        Ok(arr) => arr,
        Err(_) => return (Vec::new(), Vec::new(), own_ops_only),
    };

    let mut sessions = Vec::new();
    let mut slow = Vec::new();
    let min = min_ms as i64;

    for item in inprog {
        let Some(op) = item.as_document() else {
            continue;
        };
        let id = op
            .get("opid")
            .map(opid_string)
            .unwrap_or_else(|| "?".into());
        let secs = bson_f64(op.get("secs_running")).unwrap_or(0.0);
        let duration_ms = (secs * 1000.0).round() as i64;
        let ns = op.get_str("ns").ok().map(str::to_string);
        let database = ns
            .as_deref()
            .and_then(|s| s.split('.').next())
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let user = op
            .get_array("effectiveUsers")
            .ok()
            .and_then(|arr| arr.first())
            .and_then(Bson::as_document)
            .and_then(|d| d.get_str("user").ok())
            .map(str::to_string);
        let state = op.get_str("op").ok().map(str::to_string).or_else(|| {
            if op.get_bool("active").ok().unwrap_or(false) {
                Some("active".into())
            } else {
                Some("idle".into())
            }
        });
        let query = command_preview(op);
        let mut extras = Vec::new();
        if let Ok(client) = op.get_str("client") {
            extras.push(MetricValue::text("client", "Client", client));
        }
        if let Ok(app) = op.get_str("appName") {
            extras.push(MetricValue::text("application", "Application", app));
        }
        if let Some(ns) = ns.clone() {
            extras.push(MetricValue::text("ns", "Namespace", ns));
        }

        if duration_ms >= min && query.as_deref().is_some_and(|q| !q.is_empty()) {
            slow.push(SlowQuery {
                id: format!("live:{id}"),
                query: query.clone().unwrap_or_default(),
                duration_ms: Some(duration_ms),
                mean_ms: None,
                calls: None,
                database: database.clone(),
                user: user.clone(),
                source: Some("live".into()),
                extras: vec![],
            });
        }

        sessions.push(ActiveBackendSession {
            id,
            user,
            database,
            state,
            duration_ms: Some(duration_ms),
            query,
            can_terminate: !own_ops_only,
            extras,
        });
    }

    (sessions, slow, own_ops_only)
}

async fn load_current_op(
    session: &MongoSession,
    db_name: Option<&str>,
) -> Option<(Document, bool)> {
    let admin = session.client.database("admin");
    match admin
        .run_command(doc! { "currentOp": 1, "$all": true })
        .await
    {
        Ok(doc) => return Some((doc, false)),
        Err(e) if !is_unauthorized(&e) => return None,
        Err(_) => {}
    }
    if let Ok(doc) = admin
        .run_command(doc! { "currentOp": 1, "$ownOps": true })
        .await
    {
        return Some((doc, true));
    }
    if let Some(db) = db_name {
        if let Ok(doc) = session
            .client
            .database(db)
            .run_command(doc! { "currentOp": 1, "$ownOps": true })
            .await
        {
            return Some((doc, true));
        }
    }
    None
}

async fn list_profile(session: &MongoSession, limit: u32) -> ConnectorResult<Vec<SlowQuery>> {
    let db_name = match session.resolve_database(None) {
        Ok(n) => n,
        Err(_) => return Ok(Vec::new()),
    };
    let coll = session
        .client
        .database(&db_name)
        .collection::<Document>("system.profile");
    let mut cursor = coll
        .find(doc! {})
        .sort(doc! { "millis": -1 })
        .limit(i64::from(limit))
        .await
        .map_err(query_failed)?;

    let mut out = Vec::new();
    let mut i = 0usize;
    while cursor.advance().await.map_err(query_failed)? {
        let d = cursor.deserialize_current().map_err(query_failed)?;
        let millis = bson_f64(d.get("millis")).unwrap_or(0.0);
        let query = d
            .get("command")
            .and_then(|c| serde_json::to_string(c).ok())
            .or_else(|| d.get_str("op").ok().map(str::to_string))
            .unwrap_or_default();
        out.push(SlowQuery {
            id: format!("hist:{i}"),
            query,
            duration_ms: Some(millis.round() as i64),
            mean_ms: None,
            calls: None,
            database: Some(db_name.clone()),
            user: None,
            source: Some("historical".into()),
            extras: vec![],
        });
        i += 1;
    }
    Ok(out)
}

fn wired_tiger_cache(status: &Document) -> (Option<f64>, Option<f64>) {
    let cache = status
        .get_document("wiredTiger")
        .ok()
        .and_then(|wt| wt.get_document("cache").ok());
    let Some(cache) = cache else {
        return (None, None);
    };
    let requested = bson_f64(cache.get("pages requested from the cache"));
    let read = bson_f64(cache.get("pages read into cache"));
    let bytes = bson_f64(cache.get("bytes currently in the cache"));
    let ratio = match (requested, read) {
        (Some(r), Some(d)) if r > 0.0 => Some(((r - d) / r).clamp(0.0, 1.0)),
        _ => None,
    };
    (ratio, bytes)
}

fn bson_f64(v: Option<&Bson>) -> Option<f64> {
    match v? {
        Bson::Double(n) => Some(*n),
        Bson::Int32(n) => Some(*n as f64),
        Bson::Int64(n) => Some(*n as f64),
        Bson::Decimal128(d) => d.to_string().parse().ok(),
        _ => None,
    }
}

fn opid_string(v: &Bson) -> String {
    match v {
        Bson::Int32(n) => n.to_string(),
        Bson::Int64(n) => n.to_string(),
        Bson::String(s) => s.clone(),
        Bson::Document(d) => serde_json::to_string(d).unwrap_or_else(|_| d.to_string()),
        other => other.to_string(),
    }
}

fn parse_opid(id: &str) -> Bson {
    if let Ok(n) = id.parse::<i64>() {
        return Bson::Int64(n);
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(id) {
        if let Ok(b) = mongodb::bson::to_bson(&v) {
            return b;
        }
    }
    Bson::String(id.to_string())
}

fn command_preview(op: &Document) -> Option<String> {
    if let Ok(cmd) = op.get_document("command") {
        return serde_json::to_string(cmd).ok();
    }
    if let Some(q) = op.get("query") {
        return serde_json::to_string(q).ok();
    }
    op.get_str("op").ok().map(str::to_string)
}
