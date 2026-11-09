//! F-01: the agent and MCP are their own principals.
//!
//! Their sessions (`agent:{profile}`, `mcp:{profile}`) are separate pools,
//! never a query tab's session and never a tab's pinned transaction. A human
//! write unlock is keyed by the human session, so it cannot reach them, and
//! a tool session can never be unlocked or pinned. A tool write passes the
//! write guard only with a one-shot authorization for its exact text, which
//! `AgentTools` issues after the access-level decision (F-02).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use mystique_connector_api::{ConnectorError, ConnectorResult};

use super::{ConnectionManager, SessionOrigin};

const TOOL_PREFIXES: [&str; 2] = ["agent:", "mcp:"];

/// Why an agent or MCP write was refused at the guard.
pub const TOOL_WRITE_MESSAGE: &str = "Agent and MCP writes run only through Mystique's access levels (Settings → MCP → Access). A write unlock or open transaction in a query tab never applies to them.";

/// `agent:` / `mcp:` ids belong to tool principals.
pub(crate) fn is_tool_session_id(session_id: &str) -> bool {
    let id = session_id.trim();
    TOOL_PREFIXES.iter().any(|prefix| id.starts_with(prefix))
}

struct Slot {
    token: u64,
    text: String,
    used: bool,
}

/// Outstanding one-shot authorizations, per tool session.
#[derive(Default)]
pub(super) struct ToolWrites {
    next: AtomicU64,
    slots: Mutex<HashMap<String, Vec<Slot>>>,
}

/// Held by `AgentTools` around one execute. Dropping it withdraws the
/// authorization, used or not.
pub(crate) struct ToolWriteAuth<'a> {
    writes: &'a ToolWrites,
    session_id: String,
    token: u64,
}

impl Drop for ToolWriteAuth<'_> {
    fn drop(&mut self) {
        if let Ok(mut map) = self.writes.slots.lock() {
            if let Some(list) = map.get_mut(&self.session_id) {
                list.retain(|slot| slot.token != self.token);
                if list.is_empty() {
                    map.remove(&self.session_id);
                }
            }
        }
    }
}

impl ConnectionManager {
    /// Allow one write of exactly `text` on this tool session.
    pub(crate) fn authorize_tool_write(&self, session_id: &str, text: &str) -> ToolWriteAuth<'_> {
        let token = self.tool_writes.next.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut map) = self.tool_writes.slots.lock() {
            map.entry(session_id.to_string()).or_default().push(Slot {
                token,
                text: text.to_string(),
                used: false,
            });
        }
        ToolWriteAuth {
            writes: &self.tool_writes,
            session_id: session_id.to_string(),
            token,
        }
    }

    /// Spend an authorization for exactly this text. One use.
    pub(super) fn take_tool_write(&self, session_id: &str, text: &str) -> bool {
        let Ok(mut map) = self.tool_writes.slots.lock() else {
            return false;
        };
        let Some(slot) = map
            .get_mut(session_id)
            .and_then(|list| list.iter_mut().find(|s| !s.used && s.text == text))
        else {
            return false;
        };
        slot.used = true;
        true
    }

    /// A tool principal's session: by id, or by who opened it.
    pub(super) async fn is_tool_session(&self, session_id: &str) -> bool {
        if is_tool_session_id(session_id) {
            return true;
        }
        self.sessions
            .read()
            .await
            .get(session_id)
            .is_some_and(|live| live.origin != SessionOrigin::Ui)
    }

    /// Unlock and pin are human actions.
    pub(super) async fn refuse_tool_session(
        &self,
        session_id: &str,
        what: &str,
    ) -> ConnectorResult<()> {
        if self.is_tool_session(session_id).await {
            return Err(ConnectorError::Policy(format!(
                "Agent and MCP sessions cannot be {what}; their access is set in Settings → MCP → Access."
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_ids_are_recognised() {
        assert!(is_tool_session_id("mcp:12"));
        assert!(is_tool_session_id(" agent:3"));
        assert!(!is_tool_session_id("12"));
        assert!(!is_tool_session_id("mcp"));
    }

    #[test]
    fn an_authorization_is_one_use_and_exact() {
        let mgr = ConnectionManager::bootstrap();
        let auth = mgr.authorize_tool_write("mcp:1", "UPDATE t SET a = 1");
        assert!(!mgr.take_tool_write("mcp:1", "UPDATE t SET a = 2"));
        assert!(!mgr.take_tool_write("mcp:2", "UPDATE t SET a = 1"));
        assert!(!mgr.take_tool_write("1", "UPDATE t SET a = 1"));
        assert!(mgr.take_tool_write("mcp:1", "UPDATE t SET a = 1"));
        assert!(
            !mgr.take_tool_write("mcp:1", "UPDATE t SET a = 1"),
            "used twice"
        );
        drop(auth);
        let unused = mgr.authorize_tool_write("mcp:1", "DELETE FROM t");
        drop(unused);
        assert!(
            !mgr.take_tool_write("mcp:1", "DELETE FROM t"),
            "withdrawn on drop"
        );
    }
}
