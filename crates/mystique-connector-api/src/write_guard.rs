//! Write confirmation prompt (see [`crate::confirmation_for`]).
//!
//! Travels to the UI as `IpcPrompt::WriteGuard` in the IPC error envelope
//! (`crate::IpcError`). `Display` is JSON for MCP and logs.

use serde::{Deserialize, Serialize};

use crate::Confirmation;

/// Always `writeGuard`. Kept on the wire for older readers.
pub const WRITE_GUARD_CODE: &str = "writeGuard";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct WriteGuardPrompt {
    pub code: String,
    /// `unlock` (allow writes this session), `confirm` (one dialog, no unlock)
    /// or `typeConfirm` (type the object name).
    pub kind: String,
    pub environment: String,
    pub connection_name: String,
    /// Short description of the write.
    pub preview: String,
    /// Statements the write will run, when known.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub statements: Vec<String>,
    /// Name to send back as `confirmObjectName` for `confirm` and `typeConfirm`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_name: Option<String>,
    /// The database marked with `environment` that the write lands in, when
    /// the mark is per database rather than the connection's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
}

impl WriteGuardPrompt {
    /// `None` when the policy asks nothing.
    pub fn from_confirmation(
        ask: Confirmation,
        environment: String,
        connection_name: String,
        preview: String,
        object_name: &str,
    ) -> Option<Self> {
        let (kind, statements, object_name) = match ask {
            Confirmation::None => return None,
            Confirmation::Sql {
                statements,
                unlock: true,
            } => ("unlock", statements, None),
            Confirmation::Sql {
                statements,
                unlock: false,
            } => ("confirm", statements, Some(object_name.to_string())),
            Confirmation::TypeConfirm { name, statements } => {
                ("typeConfirm", statements, Some(name))
            }
        };
        Some(Self {
            code: WRITE_GUARD_CODE.into(),
            kind: kind.into(),
            environment,
            connection_name,
            preview,
            statements,
            object_name,
            database: None,
        })
    }

    /// Human text for the IPC envelope `message`.
    pub fn summary(&self) -> String {
        format!(
            "Confirm the write on {} ({}).",
            self.connection_name, self.environment
        )
    }
}

impl std::fmt::Display for WriteGuardPrompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match serde_json::to_string(self) {
            Ok(s) => f.write_str(&s),
            Err(_) => write!(
                f,
                "Confirm write on {} ({})",
                self.connection_name, self.environment
            ),
        }
    }
}

/// How dangerous a statement is. SQL uses the AST in `mystique-sql`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WriteRisk {
    Read = 0,
    Write = 1,
    Destructive = 2,
}
