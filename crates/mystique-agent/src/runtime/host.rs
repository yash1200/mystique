//! What the loop needs from the app, behind traits so tests can fake it:
//! the tools (in-process [`mystique_core::AgentTools`], never MCP over HTTP),
//! where tool cards are saved, and the approval dock's queue.

use std::sync::Arc;

use mystique_core::{AppServices, AppendAgentMessageInput, ToolApprovalRequest, ToolCaller};
use rig::completion::ToolDefinition;
use serde_json::Value as JsonValue;

use crate::tools::{tool_definitions, BoundTools};

use super::draft::extract_draft;
use super::model::BoxFuture;
use super::prompt::ErrorAt;

/// A failed tool call. `at` points into the statement it ran, if any.
#[derive(Debug, Clone)]
pub(crate) struct ToolFailure {
    pub message: String,
    pub at: Option<ErrorAt>,
}

pub(crate) trait ToolRunner: Send + Sync {
    fn definitions(&self) -> Vec<ToolDefinition>;
    fn call<'a>(
        &'a self,
        name: &'a str,
        args: JsonValue,
    ) -> BoxFuture<'a, Result<JsonValue, ToolFailure>>;
}

impl ToolRunner for BoundTools {
    fn definitions(&self) -> Vec<ToolDefinition> {
        tool_definitions(self)
    }

    fn call<'a>(
        &'a self,
        name: &'a str,
        args: JsonValue,
    ) -> BoxFuture<'a, Result<JsonValue, ToolFailure>> {
        Box::pin(async move {
            self.dispatch_located(name, args)
                .await
                .map_err(|e| ToolFailure {
                    message: e.message,
                    at: e.at.map(|(line, column)| ErrorAt { line, column }),
                })
        })
    }
}

/// Persistence and the approval queue, as the loop sees them.
pub(crate) trait RunHost: Send + Sync {
    /// Save a tool card. Returns its message id.
    fn save_tool(&self, name: &str, content: &str) -> Option<String>;
    /// Save the assistant reply (with its draft). Returns its message id.
    fn save_reply(&self, content: &str) -> Result<String, String>;
    /// Approval requests this run's connection has waiting in the dock.
    fn pending_approvals(&self) -> Vec<ToolApprovalRequest>;
}

/// Where one chat's run saves its rows, and which dock requests are its own.
///
/// The built-in agent calls tools as [`ToolCaller::Agent`]. An external
/// (ACP) agent reaches them through the MCP server, so its dock requests
/// carry [`ToolCaller::Mcp`].
#[derive(Clone)]
pub struct ChatHost {
    pub app: Arc<AppServices>,
    pub chat_id: String,
    pub connection_id: String,
    pub caller: ToolCaller,
}

impl ChatHost {
    /// Save a tool card. Returns its message id.
    pub fn save_tool(&self, name: &str, content: &str) -> Option<String> {
        RunHost::save_tool(self, name, content)
    }

    /// Save the assistant reply (with its draft). Returns its message id.
    pub fn save_reply(&self, content: &str) -> Result<String, String> {
        RunHost::save_reply(self, content)
    }

    /// Approval requests this caller has waiting on the run's connection.
    pub fn pending_approvals(&self) -> Vec<ToolApprovalRequest> {
        RunHost::pending_approvals(self)
    }
}

impl RunHost for ChatHost {
    fn save_tool(&self, name: &str, content: &str) -> Option<String> {
        match self
            .app
            .agent_chats()
            .append_message(AppendAgentMessageInput {
                chat_id: self.chat_id.clone(),
                role: "tool".into(),
                content: content.to_string(),
                tool_name: Some(name.to_string()),
                draft_body: None,
            }) {
            Ok(row) => Some(row.id),
            Err(err) => {
                tracing::warn!(error = %err, "could not save agent tool card");
                None
            }
        }
    }

    fn save_reply(&self, content: &str) -> Result<String, String> {
        self.app
            .agent_chats()
            .append_message(AppendAgentMessageInput {
                chat_id: self.chat_id.clone(),
                role: "assistant".into(),
                content: content.to_string(),
                tool_name: None,
                draft_body: extract_draft(content),
            })
            .map(|row| row.id)
            .map_err(|e| e.to_string())
    }

    fn pending_approvals(&self) -> Vec<ToolApprovalRequest> {
        self.app
            .tool_approvals()
            .pending()
            .into_iter()
            .filter(|r| r.caller == self.caller.as_str() && r.connection_id == self.connection_id)
            .collect()
    }
}
