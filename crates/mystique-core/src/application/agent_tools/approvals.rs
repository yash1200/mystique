//! The in-app approval dock's queue (F-02, "ask me for high-risk writes").
//!
//! A tool call that is above its access level, on a connection that asks,
//! waits here for the user. Approve once issues a [`ToolGrant`] for exactly
//! that text, connection, database and caller; the waiting call consumes it
//! and runs. Deny, a closed client, or the timeout (90 s) refuse it. Nothing
//! is persisted, so no request or grant survives a restart.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use tokio::sync::oneshot;

use crate::domain::ToolApprovalRequest;
use crate::error::{AppError, AppResult};

use super::levels::{AccessLevel, ToolCaller};

/// How long a request waits before it is denied.
pub const APPROVAL_TIMEOUT: Duration = Duration::from_secs(90);

/// What the user is asked about. The dock shows all of it.
#[derive(Debug, Clone)]
pub struct ApprovalAsk {
    pub caller: ToolCaller,
    pub connection_id: String,
    pub connection_name: String,
    pub database: Option<String>,
    pub environment: String,
    pub sql: String,
    pub needed: AccessLevel,
    pub granted: AccessLevel,
}

/// Single use: exactly one statement text, on one connection and database,
/// for one caller. It is moved into the call that asked and spent there.
#[derive(Debug, PartialEq, Eq)]
pub struct ToolGrant {
    caller: ToolCaller,
    connection_id: String,
    database: Option<String>,
    sql: String,
}

impl ToolGrant {
    /// Byte-for-byte: approving A never allows B.
    pub fn covers(
        &self,
        caller: ToolCaller,
        connection_id: &str,
        database: Option<&str>,
        sql: &str,
    ) -> bool {
        self.caller == caller
            && self.connection_id == connection_id
            && self.database.as_deref() == database
            && self.sql == sql
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Approved(ToolGrant),
    Denied,
    TimedOut,
}

struct Pending {
    request: ToolApprovalRequest,
    answer: oneshot::Sender<bool>,
}

type Notifier = Arc<dyn Fn() + Send + Sync>;

/// Pending requests. One per process, owned by `AppServices`.
pub struct ToolApprovals {
    pending: Mutex<HashMap<String, Pending>>,
    notify: RwLock<Option<Notifier>>,
    timeout: Mutex<Duration>,
}

impl Default for ToolApprovals {
    fn default() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            notify: RwLock::new(None),
            timeout: Mutex::new(APPROVAL_TIMEOUT),
        }
    }
}

/// Takes the request out of the queue however the wait ends, including
/// when the MCP client gives up and the call is dropped.
struct Leave<'a> {
    queue: &'a ToolApprovals,
    id: String,
}

impl Drop for Leave<'_> {
    fn drop(&mut self) {
        let removed = self
            .queue
            .pending
            .lock()
            .map(|mut map| map.remove(&self.id).is_some())
            .unwrap_or(false);
        if removed {
            self.queue.changed();
        }
    }
}

impl ToolApprovals {
    /// The shell emits an event so the dock refreshes.
    pub fn set_notifier(&self, notify: impl Fn() + Send + Sync + 'static) {
        if let Ok(mut slot) = self.notify.write() {
            *slot = Some(Arc::new(notify));
        }
    }

    /// Tests shorten the wait. The app keeps [`APPROVAL_TIMEOUT`].
    pub fn set_timeout(&self, timeout: Duration) {
        if let Ok(mut slot) = self.timeout.lock() {
            *slot = timeout;
        }
    }

    fn timeout(&self) -> Duration {
        self.timeout.lock().map(|t| *t).unwrap_or(APPROVAL_TIMEOUT)
    }

    fn changed(&self) {
        let notify = self.notify.read().ok().and_then(|slot| slot.clone());
        if let Some(notify) = notify {
            notify();
        }
    }

    /// Oldest first.
    pub fn pending(&self) -> Vec<ToolApprovalRequest> {
        let mut list: Vec<ToolApprovalRequest> = self
            .pending
            .lock()
            .map(|map| map.values().map(|p| p.request.clone()).collect())
            .unwrap_or_default();
        list.sort_by(|a, b| a.requested_at.cmp(&b.requested_at));
        list
    }

    /// Approve once or deny. A request that already ended is not found.
    pub fn decide(&self, id: &str, approve: bool) -> AppResult<()> {
        let pending = self
            .pending
            .lock()
            .map_err(|_| AppError::Internal("approval queue poisoned".into()))?
            .remove(id)
            .ok_or_else(|| {
                AppError::NotFound("This request already ended (answered or timed out).".into())
            })?;
        let _ = pending.answer.send(approve);
        self.changed();
        Ok(())
    }

    /// Queue the request and wait for the user, at most the timeout.
    pub async fn request(&self, ask: ApprovalAsk) -> ApprovalOutcome {
        let timeout = self.timeout();
        let now = chrono::Utc::now();
        let expires = now + chrono::Duration::from_std(timeout).unwrap_or_default();
        let id = uuid::Uuid::new_v4().to_string();
        let request = ToolApprovalRequest {
            id: id.clone(),
            caller: ask.caller.as_str().into(),
            connection_id: ask.connection_id.clone(),
            connection_name: ask.connection_name,
            database: ask.database.clone(),
            environment: ask.environment,
            sql: ask.sql.clone(),
            needed: ask.needed.as_str().into(),
            granted: ask.granted.as_str().into(),
            requested_at: now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            expires_at: expires.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        };
        let (tx, rx) = oneshot::channel();
        if let Ok(mut map) = self.pending.lock() {
            map.insert(
                id.clone(),
                Pending {
                    request,
                    answer: tx,
                },
            );
        } else {
            return ApprovalOutcome::Denied;
        }
        let _leave = Leave { queue: self, id };
        self.changed();
        tracing::info!(
            caller = ask.caller.as_str(),
            "tool write waiting for approval"
        );
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(true)) => ApprovalOutcome::Approved(ToolGrant {
                caller: ask.caller,
                connection_id: ask.connection_id,
                database: ask.database,
                sql: ask.sql,
            }),
            Ok(_) => ApprovalOutcome::Denied,
            Err(_) => ApprovalOutcome::TimedOut,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(sql: &str) -> ApprovalAsk {
        ApprovalAsk {
            caller: ToolCaller::Mcp,
            connection_id: "1".into(),
            connection_name: "prod".into(),
            database: Some("app".into()),
            environment: "production".into(),
            sql: sql.into(),
            needed: AccessLevel::Full,
            granted: AccessLevel::Read,
        }
    }

    async fn first_pending(queue: &ToolApprovals) -> ToolApprovalRequest {
        for _ in 0..200 {
            if let Some(req) = queue.pending().into_iter().next() {
                return req;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("nothing was queued");
    }

    #[tokio::test]
    async fn approve_once_grants_exactly_that_statement() {
        let queue = Arc::new(ToolApprovals::default());
        let waiting = {
            let queue = Arc::clone(&queue);
            tokio::spawn(async move { queue.request(ask("DELETE FROM t WHERE id = 1")).await })
        };
        let req = first_pending(&queue).await;
        assert_eq!(req.sql, "DELETE FROM t WHERE id = 1");
        assert_eq!(req.environment, "production");
        assert_eq!(req.needed, "full");
        queue.decide(&req.id, true).unwrap();
        let ApprovalOutcome::Approved(grant) = waiting.await.unwrap() else {
            panic!("expected a grant");
        };
        assert!(grant.covers(
            ToolCaller::Mcp,
            "1",
            Some("app"),
            "DELETE FROM t WHERE id = 1"
        ));
        // A, not B: another text, database, connection or caller.
        assert!(!grant.covers(ToolCaller::Mcp, "1", Some("app"), "DELETE FROM t"));
        assert!(!grant.covers(
            ToolCaller::Mcp,
            "1",
            Some("app"),
            "DELETE FROM t WHERE id = 1 "
        ));
        assert!(!grant.covers(ToolCaller::Mcp, "1", None, "DELETE FROM t WHERE id = 1"));
        assert!(!grant.covers(
            ToolCaller::Mcp,
            "2",
            Some("app"),
            "DELETE FROM t WHERE id = 1"
        ));
        assert!(!grant.covers(
            ToolCaller::Agent,
            "1",
            Some("app"),
            "DELETE FROM t WHERE id = 1"
        ));
        assert!(queue.pending().is_empty());
        // Answered: a second decision finds nothing.
        assert!(queue.decide(&req.id, true).is_err());
    }

    #[tokio::test]
    async fn deny_and_timeout_refuse() {
        let queue = Arc::new(ToolApprovals::default());
        let waiting = {
            let queue = Arc::clone(&queue);
            tokio::spawn(async move { queue.request(ask("DROP TABLE t")).await })
        };
        let req = first_pending(&queue).await;
        queue.decide(&req.id, false).unwrap();
        assert_eq!(waiting.await.unwrap(), ApprovalOutcome::Denied);

        queue.set_timeout(Duration::from_millis(30));
        assert_eq!(
            queue.request(ask("DROP TABLE t")).await,
            ApprovalOutcome::TimedOut
        );
        assert!(
            queue.pending().is_empty(),
            "a timed-out request stays queued"
        );
    }

    #[tokio::test]
    async fn a_dropped_call_leaves_the_queue_and_notifies() {
        let queue = Arc::new(ToolApprovals::default());
        let seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        {
            let seen = Arc::clone(&seen);
            queue.set_notifier(move || {
                seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            });
        }
        let waiting = {
            let queue = Arc::clone(&queue);
            tokio::spawn(async move { queue.request(ask("DROP TABLE t")).await })
        };
        first_pending(&queue).await;
        waiting.abort();
        let _ = waiting.await;
        assert!(queue.pending().is_empty());
        assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 2);
    }
}
