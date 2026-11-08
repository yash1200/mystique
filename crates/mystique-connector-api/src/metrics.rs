//! Engine-agnostic server metrics contract.
//!
//! Every plugin returns the same snapshot. Unsupported fields stay
//! `available: false` with a reason. Engine-specific extras go on
//! `overview.extras` and row `extras` so new engines do not need a
//! contract change.

use serde::{Deserialize, Serialize};

/// Soft cap when the client omits `slowQueryLimit`.
pub const DEFAULT_SLOW_QUERY_LIMIT: u32 = 50;
/// Hard ceiling applied by core on every request.
pub const HARD_SLOW_QUERY_LIMIT: u32 = 200;
/// Live slow-query threshold when the client omits `minDurationMs`.
pub const DEFAULT_MIN_DURATION_MS: u64 = 1_000;

fn default_true() -> bool {
    true
}

/// What this engine can populate in the metrics tab.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ServerMetricsCapabilities {
    pub overview: bool,
    pub active_sessions: bool,
    pub terminate_session: bool,
    pub slow_queries: bool,
    /// Human notes (missing extensions, file-engine limits, …).
    #[serde(default)]
    pub notes: Vec<String>,
}

impl ServerMetricsCapabilities {
    pub fn full() -> Self {
        Self {
            overview: true,
            active_sessions: true,
            terminate_session: true,
            slow_queries: true,
            notes: Vec::new(),
        }
    }

    pub fn with_notes(mut self, notes: Vec<String>) -> Self {
        self.notes = notes;
        self
    }
}

/// How the UI should render a metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum MetricKind {
    #[default]
    Count,
    Bytes,
    DurationMs,
    Percent,
    Text,
    Ratio,
}

/// One typed metric. `value` is raw; `display` is an optional plugin hint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct MetricValue {
    pub key: String,
    pub label: String,
    pub kind: MetricKind,
    #[serde(default)]
    pub value: Option<f64>,
    #[serde(default)]
    pub display: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    /// Gauge max (e.g. max connections).
    #[serde(default)]
    pub max: Option<f64>,
    pub available: bool,
    #[serde(default)]
    pub unavailable_reason: Option<String>,
}

impl MetricValue {
    pub fn count(key: impl Into<String>, label: impl Into<String>, value: f64) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind: MetricKind::Count,
            value: Some(value),
            display: None,
            unit: None,
            max: None,
            available: true,
            unavailable_reason: None,
        }
    }

    pub fn count_of(
        key: impl Into<String>,
        label: impl Into<String>,
        value: f64,
        max: f64,
    ) -> Self {
        let mut m = Self::count(key, label, value);
        m.max = Some(max);
        m
    }

    pub fn bytes(key: impl Into<String>, label: impl Into<String>, value: f64) -> Self {
        let mut m = Self::count(key, label, value);
        m.kind = MetricKind::Bytes;
        m.unit = Some("bytes".into());
        m
    }

    pub fn duration_ms(key: impl Into<String>, label: impl Into<String>, value: f64) -> Self {
        let mut m = Self::count(key, label, value);
        m.kind = MetricKind::DurationMs;
        m.unit = Some("ms".into());
        m
    }

    /// `value` is 0–100.
    pub fn percent(key: impl Into<String>, label: impl Into<String>, value: f64) -> Self {
        let mut m = Self::count(key, label, value);
        m.kind = MetricKind::Percent;
        m.unit = Some("%".into());
        m
    }

    /// `ratio` is 0–1; stored as a 0–100 percent.
    pub fn ratio_as_percent(key: impl Into<String>, label: impl Into<String>, ratio: f64) -> Self {
        Self::percent(key, label, (ratio * 100.0).clamp(0.0, 100.0))
    }

    pub fn text(
        key: impl Into<String>,
        label: impl Into<String>,
        display: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind: MetricKind::Text,
            value: None,
            display: Some(display.into()),
            unit: None,
            max: None,
            available: true,
            unavailable_reason: None,
        }
    }

    pub fn unavailable(
        key: impl Into<String>,
        label: impl Into<String>,
        kind: MetricKind,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind,
            value: None,
            display: None,
            unit: None,
            max: None,
            available: false,
            unavailable_reason: Some(reason.into()),
        }
    }
}

/// Headline server snapshot. Keys are stable so the FE can layout them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ServerOverview {
    pub connections: MetricValue,
    pub cache_hit_ratio: MetricValue,
    pub database_size: MetricValue,
    pub uptime: MetricValue,
    pub active_queries: MetricValue,
    /// Engine-specific extras (WAL mode, WiredTiger cache, Questions, …).
    #[serde(default)]
    pub extras: Vec<MetricValue>,
}

/// One live backend session / connection / operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ActiveBackendSession {
    /// Engine-native id (`pid`, MySQL `Id`, Mongo `opid`).
    pub id: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub query: Option<String>,
    pub can_terminate: bool,
    #[serde(default)]
    pub extras: Vec<MetricValue>,
}

/// A currently-running or historically slow statement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SlowQuery {
    pub id: String,
    pub query: String,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub mean_ms: Option<f64>,
    #[serde(default)]
    pub calls: Option<i64>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    /// `live` (running now) or `historical` (pg_stat_statements / digest).
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub extras: Vec<MetricValue>,
}

/// Full metrics payload returned to the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ServerMetricsSnapshot {
    pub engine: String,
    pub collected_at: String,
    pub capabilities: ServerMetricsCapabilities,
    pub overview: ServerOverview,
    #[serde(default)]
    pub sessions: Vec<ActiveBackendSession>,
    #[serde(default)]
    pub slow_queries: Vec<SlowQuery>,
}

/// IPC request. `sessionId` is the Mystique pool session, not a backend pid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct FetchServerMetricsRequest {
    pub session_id: String,
    #[serde(default = "default_true")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub include_sessions: bool,
    #[serde(default = "default_true")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub include_slow_queries: bool,
    #[serde(default)]
    pub slow_query_limit: Option<u32>,
    #[serde(default)]
    pub min_duration_ms: Option<u64>,
}

/// Validated payload handed to the plugin.
#[derive(Debug, Clone)]
pub struct FetchServerMetricsInput {
    pub include_sessions: bool,
    pub include_slow_queries: bool,
    pub slow_query_limit: u32,
    pub min_duration_ms: u64,
}

impl FetchServerMetricsRequest {
    pub fn into_input(self) -> FetchServerMetricsInput {
        FetchServerMetricsInput {
            include_sessions: self.include_sessions,
            include_slow_queries: self.include_slow_queries,
            slow_query_limit: self
                .slow_query_limit
                .unwrap_or(DEFAULT_SLOW_QUERY_LIMIT)
                .clamp(1, HARD_SLOW_QUERY_LIMIT),
            min_duration_ms: self.min_duration_ms.unwrap_or(DEFAULT_MIN_DURATION_MS),
        }
    }
}

/// IPC request to kill a backend session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct TerminateSessionRequest {
    pub session_id: String,
    /// Engine-native id from [`ActiveBackendSession::id`].
    pub backend_session_id: String,
}

/// Validated payload handed to the plugin.
#[derive(Debug, Clone)]
pub struct TerminateSessionInput {
    pub backend_session_id: String,
}

impl TerminateSessionRequest {
    pub fn into_input(self) -> TerminateSessionInput {
        TerminateSessionInput {
            backend_session_id: self.backend_session_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct TerminateSessionResult {
    pub ok: bool,
    pub backend_session_id: String,
    #[serde(default)]
    pub message: Option<String>,
}

impl TerminateSessionResult {
    pub fn ok(id: impl Into<String>) -> Self {
        Self {
            ok: true,
            backend_session_id: id.into(),
            message: None,
        }
    }

    pub fn failed(id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            backend_session_id: id.into(),
            message: Some(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_serializes_camel_case() {
        let snap = ServerMetricsSnapshot {
            engine: "postgres".into(),
            collected_at: "2026-01-01T00:00:00Z".into(),
            capabilities: ServerMetricsCapabilities::full(),
            overview: ServerOverview {
                connections: MetricValue::count_of("connections", "Connections", 12.0, 100.0),
                cache_hit_ratio: MetricValue::percent("cacheHitRatio", "Cache hit", 99.2),
                database_size: MetricValue::bytes("databaseSize", "Database size", 1_048_576.0),
                uptime: MetricValue::duration_ms("uptime", "Uptime", 86_400_000.0),
                active_queries: MetricValue::count("activeQueries", "Active queries", 3.0),
                extras: vec![MetricValue::text("version", "Version", "16.2")],
            },
            sessions: vec![ActiveBackendSession {
                id: "42".into(),
                user: Some("app".into()),
                database: Some("app".into()),
                state: Some("active".into()),
                duration_ms: Some(1200),
                query: Some("SELECT 1".into()),
                can_terminate: true,
                extras: vec![],
            }],
            slow_queries: vec![],
        };
        let v = serde_json::to_value(&snap).unwrap();
        assert!(v.get("cacheHitRatio").is_none());
        assert!(v["overview"].get("cacheHitRatio").is_some());
        assert_eq!(
            v["overview"]["cacheHitRatio"]["available"].as_bool(),
            Some(true)
        );
        assert_eq!(v["sessions"][0]["canTerminate"].as_bool(), Some(true));
        assert_eq!(v["sessions"][0]["durationMs"].as_i64(), Some(1200));
    }

    #[test]
    fn request_clamps_slow_query_limit() {
        let req = FetchServerMetricsRequest {
            session_id: "s".into(),
            include_sessions: true,
            include_slow_queries: true,
            slow_query_limit: Some(9_999),
            min_duration_ms: None,
        };
        let input = req.into_input();
        assert_eq!(input.slow_query_limit, HARD_SLOW_QUERY_LIMIT);
        assert_eq!(input.min_duration_ms, DEFAULT_MIN_DURATION_MS);
    }

    #[test]
    fn ratio_as_percent_clamps() {
        let m = MetricValue::ratio_as_percent("cache", "Cache", 1.25);
        assert_eq!(m.value, Some(100.0));
        let m = MetricValue::ratio_as_percent("cache", "Cache", -0.2);
        assert_eq!(m.value, Some(0.0));
    }
}
