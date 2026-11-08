//! Write confirmation prompt (see [`crate::confirmation_for`]).
//!
//! Serialized as JSON in [`ConnectorError::WriteGuard`] so the UI can parse
//! the IPC error string the same way as SSH host-key prompts.

use serde::{Deserialize, Serialize};

use crate::Confirmation;

/// Always `writeGuard` so the UI can JSON.parse IPC errors.
pub const WRITE_GUARD_CODE: &str = "writeGuard";

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub statements: Vec<String>,
    /// Name to send back as `confirmObjectName` for `confirm` and `typeConfirm`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_name: Option<String>,
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
        })
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
