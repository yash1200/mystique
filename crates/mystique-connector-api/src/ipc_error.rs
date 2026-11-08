//! The one error envelope every Tauri command rejects with.
//!
//! `message` is human text with no machine prefix. The UI branches on
//! `kind` and `prompt`, never on the text. MCP and the agent keep using
//! [`ConnectorError`]'s `Display`, so their wire text does not change.

use serde::{Deserialize, Serialize};

use crate::{ConnectorError, PinStatus, SourcePosition, SshHostKeyPrompt, WriteGuardPrompt};

/// Stable machine kind of an IPC failure. snake_case on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpcErrorKind {
    InvalidConfig,
    Auth,
    Connect,
    Pool,
    Timeout,
    NotSupported,
    Query,
    /// A policy refused the request (read-only, Read mode, empty query, …).
    Policy,
    /// The write needs the user's confirmation; `prompt` carries the dialog.
    WriteGuard,
    /// The SSH bastion key is unknown or changed; `prompt` carries it.
    SshHostKey,
    /// The profile does not save its password and none is held this session.
    PasswordRequired,
    /// A grid undo was refused: the rows changed after the save.
    RollbackStale,
    NotFound,
    InvalidInput,
    /// The app's metadata store, a file, or JSON the app reads.
    Storage,
    SecretStore,
    Internal,
}

/// What the UI asks before it retries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum IpcPrompt {
    WriteGuard(WriteGuardPrompt),
    SshHostKey(SshHostKeyPrompt),
    PasswordRequired,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub kind: IpcErrorKind,
    pub message: String,
    /// Where a query fault sits in the submitted text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<SourcePosition>,
    /// 0-based statement of a failed batch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement_index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<IpcPrompt>,
    /// A manual-commit tab's transaction state after the failed run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transaction_outcome: Option<PinStatus>,
}

impl IpcError {
    pub fn new(kind: IpcErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            position: None,
            statement_index: None,
            prompt: None,
            transaction_outcome: None,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(IpcErrorKind::Internal, message)
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(IpcErrorKind::InvalidInput, message)
    }

    pub fn with_transaction_outcome(mut self, outcome: PinStatus) -> Self {
        self.transaction_outcome = Some(outcome);
        self
    }
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for IpcError {}

/// Read-mode refusals keep this prefix in `Display` for MCP and the agent.
const POLICY_PREFIX: &str = "policy: ";

impl From<ConnectorError> for IpcError {
    fn from(err: ConnectorError) -> Self {
        match err {
            ConnectorError::QueryAt {
                message,
                position,
                statement_index,
            } => {
                let (kind, message) = match message.strip_prefix(POLICY_PREFIX) {
                    Some(rest) => (IpcErrorKind::Policy, rest.to_string()),
                    None => (IpcErrorKind::Query, message),
                };
                Self {
                    position: Some(position),
                    statement_index,
                    ..Self::new(kind, message)
                }
            }
            ConnectorError::Policy(message) => Self::new(IpcErrorKind::Policy, message),
            ConnectorError::Query(message)
                if message.starts_with(crate::ROLLBACK_STALE)
                    && message[crate::ROLLBACK_STALE.len()..].starts_with(": ") =>
            {
                let rest = &message[crate::ROLLBACK_STALE.len() + 2..];
                Self::new(IpcErrorKind::RollbackStale, rest)
            }
            ConnectorError::WriteGuard(prompt) => Self {
                prompt: Some(IpcPrompt::WriteGuard(prompt.clone())),
                ..Self::new(IpcErrorKind::WriteGuard, prompt.summary())
            },
            ConnectorError::SshHostKey(prompt) => Self {
                prompt: Some(IpcPrompt::SshHostKey(prompt.clone())),
                ..Self::new(IpcErrorKind::SshHostKey, prompt.summary())
            },
            other => {
                let kind = match &other {
                    ConnectorError::InvalidConfig(_) => IpcErrorKind::InvalidConfig,
                    ConnectorError::Auth(_) => IpcErrorKind::Auth,
                    ConnectorError::Connect(_) => IpcErrorKind::Connect,
                    ConnectorError::Pool(_) => IpcErrorKind::Pool,
                    ConnectorError::Timeout(_) => IpcErrorKind::Timeout,
                    ConnectorError::NotSupported(_) => IpcErrorKind::NotSupported,
                    ConnectorError::Query(_) => IpcErrorKind::Query,
                    _ => IpcErrorKind::Internal,
                };
                Self::new(kind, other.to_string())
            }
        }
    }
}

/// Plain-string failures from the shell itself (panics, path lookups).
impl From<String> for IpcError {
    fn from(message: String) -> Self {
        Self::internal(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wire(err: ConnectorError) -> serde_json::Value {
        serde_json::to_value(IpcError::from(err)).expect("serialize")
    }

    #[test]
    fn policy_drops_the_prefix_and_sets_the_kind() {
        let value = wire(ConnectorError::Policy(
            "This connection is read-only.".into(),
        ));
        assert_eq!(
            value,
            json!({ "kind": "policy", "message": "This connection is read-only." })
        );
    }

    #[test]
    fn a_read_mode_refusal_keeps_its_position() {
        let err = ConnectorError::query_at(
            "policy: Statement 2 is not a read (DELETE FROM t). Nothing ran.",
            SourcePosition {
                offset: 10,
                line: 1,
                column: 11,
            },
            Some(1),
        );
        assert!(err.to_string().starts_with("policy: "));
        let value = wire(err);
        assert_eq!(value["kind"], "policy");
        assert_eq!(
            value["message"],
            "Statement 2 is not a read (DELETE FROM t). Nothing ran."
        );
        assert_eq!(value["position"]["offset"], 10);
        assert_eq!(value["statementIndex"], 1);
    }

    #[test]
    fn a_query_fault_carries_position_and_statement() {
        let value = wire(ConnectorError::query_at(
            "syntax error at or near \"FORM\"",
            SourcePosition {
                offset: 9,
                line: 1,
                column: 10,
            },
            Some(0),
        ));
        assert_eq!(value["kind"], "query");
        assert_eq!(value["message"], "syntax error at or near \"FORM\"");
        assert_eq!(value["position"]["line"], 1);
        assert_eq!(value["statementIndex"], 0);
        assert!(value.get("prompt").is_none());
    }

    #[test]
    fn a_write_guard_travels_as_a_typed_prompt() {
        let prompt = WriteGuardPrompt::from_confirmation(
            crate::Confirmation::Sql {
                statements: vec!["DELETE FROM t WHERE id = 1".into()],
                unlock: true,
            },
            "production".into(),
            "Prod".into(),
            "DELETE FROM t WHERE id = 1".into(),
            "t",
        )
        .expect("prompt");
        let value = wire(ConnectorError::WriteGuard(prompt));
        assert_eq!(value["kind"], "write_guard");
        assert_eq!(value["prompt"]["type"], "writeGuard");
        assert_eq!(value["prompt"]["kind"], "unlock");
        assert_eq!(value["prompt"]["connectionName"], "Prod");
        let message = value["message"].as_str().expect("message");
        assert!(!message.starts_with('{'), "{message}");
    }

    #[test]
    fn an_ssh_host_key_travels_as_a_typed_prompt() {
        let prompt = SshHostKeyPrompt::unknown(
            "bastion".into(),
            22,
            "SHA256:abc".into(),
            "ssh-ed25519".into(),
            "AAAA".into(),
        );
        let value = wire(ConnectorError::SshHostKey(prompt));
        assert_eq!(value["kind"], "ssh_host_key");
        assert_eq!(value["prompt"]["type"], "sshHostKey");
        assert_eq!(value["prompt"]["fingerprint"], "SHA256:abc");
        assert!(!value["message"].as_str().unwrap_or("{").starts_with('{'));
    }

    #[test]
    fn a_stale_undo_has_its_own_kind() {
        let value = wire(crate::rollback_stale());
        assert_eq!(value["kind"], "rollback_stale");
        assert_eq!(
            value["message"],
            "The rows changed after this save. Rollback would overwrite newer data."
        );
    }

    #[test]
    fn other_errors_keep_their_text() {
        let value = wire(ConnectorError::Connect("connection refused".into()));
        assert_eq!(value["kind"], "connect");
        assert_eq!(value["message"], "connection failed: connection refused");
    }

    #[test]
    fn a_transaction_outcome_is_camel_case() {
        let err = IpcError::from(ConnectorError::Query("boom".into()))
            .with_transaction_outcome(PinStatus::pinned(true));
        let value = serde_json::to_value(err).expect("serialize");
        assert_eq!(value["transactionOutcome"]["inTransaction"], true);
    }
}
