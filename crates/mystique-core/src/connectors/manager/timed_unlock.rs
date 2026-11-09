//! Timed write unlock on a read-only connection (F-04).
//!
//! A person on a read-only profile can allow writes for one or five
//! minutes without editing the profile. The window lives here, in memory,
//! keyed by the human (UI) session id: it is never written to SQLite, a
//! disconnect or relaunch ends it, and agent and MCP sessions (their own
//! ids and origins) never see it. The backend clock decides expiry; the UI
//! only shows the remaining time it was given. A statement admitted before
//! expiry finishes; the next one is checked again.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use mystique_connector_api::{ConnectorError, ConnectorResult};
use serde::{Deserialize, Serialize};

use super::{ConnectionManager, SessionOrigin};

/// The windows the UI offers. Anything else is refused.
pub const TIMED_UNLOCK_CHOICES_SECS: [u64; 2] = [60, 300];

/// Whether writes are allowed on a read-only session, and for how long.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct TimedUnlockStatus {
    pub session_id: String,
    /// Milliseconds left; `None` when no unlock is running.
    pub remaining_ms: Option<u64>,
}

#[derive(Default)]
pub(super) struct TimedUnlocks {
    until: std::sync::Mutex<HashMap<String, Instant>>,
}

impl TimedUnlocks {
    /// Time left, dropping an expired entry.
    pub(super) fn remaining(&self, session_id: &str) -> Option<Duration> {
        let mut map = self.until.lock().ok()?;
        let until = *map.get(session_id)?;
        let left = until.checked_duration_since(Instant::now());
        match left {
            Some(left) if !left.is_zero() => Some(left),
            _ => {
                map.remove(session_id);
                None
            }
        }
    }

    pub(super) fn active(&self, session_id: &str) -> bool {
        self.remaining(session_id).is_some()
    }

    fn start(&self, session_id: &str, length: Duration) {
        if let Ok(mut map) = self.until.lock() {
            map.insert(session_id.to_string(), Instant::now() + length);
        }
    }

    pub(super) fn end(&self, session_id: &str) {
        if let Ok(mut map) = self.until.lock() {
            map.remove(session_id);
        }
    }
}

impl ConnectionManager {
    /// Allow writes on a read-only desktop session for `seconds` (60 or 300).
    pub async fn start_timed_unlock(
        &self,
        session_id: &str,
        seconds: u64,
    ) -> ConnectorResult<TimedUnlockStatus> {
        if !TIMED_UNLOCK_CHOICES_SECS.contains(&seconds) {
            return Err(ConnectorError::InvalidConfig(
                "Writes can be allowed for 1 or 5 minutes.".into(),
            ));
        }
        self.start_timed_unlock_for(session_id, Duration::from_secs(seconds))
            .await
    }

    /// Any length, for tests that cannot wait a minute.
    pub(super) async fn start_timed_unlock_for(
        &self,
        session_id: &str,
        length: Duration,
    ) -> ConnectorResult<TimedUnlockStatus> {
        let id = session_id.trim();
        {
            let guard = self.sessions.read().await;
            let live = guard
                .get(id)
                .ok_or_else(|| ConnectorError::InvalidConfig(format!("session not found: {id}")))?;
            if live.origin != SessionOrigin::Ui {
                return Err(ConnectorError::Policy(
                    "Only a person can allow writes on a read-only connection.".into(),
                ));
            }
            if !live.read_only {
                return Err(ConnectorError::InvalidConfig(
                    "This connection is not read-only.".into(),
                ));
            }
        }
        self.timed_unlocks.start(id, length);
        tracing::info!(
            session_id = id,
            seconds = length.as_secs(),
            "timed write unlock started"
        );
        Ok(self.timed_unlock_status(id))
    }

    /// End the window now.
    pub fn end_timed_unlock(&self, session_id: &str) -> TimedUnlockStatus {
        self.timed_unlocks.end(session_id.trim());
        self.timed_unlock_status(session_id)
    }

    pub fn timed_unlock_status(&self, session_id: &str) -> TimedUnlockStatus {
        let id = session_id.trim();
        TimedUnlockStatus {
            session_id: id.to_string(),
            remaining_ms: self
                .timed_unlocks
                .remaining(id)
                .map(|left| u64::try_from(left.as_millis()).unwrap_or(u64::MAX).max(1)),
        }
    }
}
