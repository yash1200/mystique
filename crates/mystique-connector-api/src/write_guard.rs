//! Production / staging write confirmation (P3.1).
//!
//! Serialized as JSON in [`ConnectorError::WriteGuard`] so the UI can parse
//! the IPC error string the same way as SSH host-key prompts.

use serde::{Deserialize, Serialize};

/// Always `writeGuard` so the UI can JSON.parse IPC errors.
pub const WRITE_GUARD_CODE: &str = "writeGuard";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteGuardPrompt {
    pub code: String,
    /// `unlock` (first write this session) or `typeConfirm` (DROP / TRUNCATE).
    pub kind: String,
    pub environment: String,
    pub connection_name: String,
    /// Statement or short description of the write.
    pub preview: String,
    /// Expected typed name for `typeConfirm`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_name: Option<String>,
}

impl WriteGuardPrompt {
    pub fn unlock(environment: String, connection_name: String, preview: String) -> Self {
        Self {
            code: WRITE_GUARD_CODE.into(),
            kind: "unlock".into(),
            environment,
            connection_name,
            preview,
            object_name: None,
        }
    }

    pub fn type_confirm(
        environment: String,
        connection_name: String,
        preview: String,
        object_name: String,
    ) -> Self {
        Self {
            code: WRITE_GUARD_CODE.into(),
            kind: "typeConfirm".into(),
            environment,
            connection_name,
            preview,
            object_name: Some(object_name),
        }
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

/// How dangerous a statement is. Keyword-level until the SQL crate (#131).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WriteRisk {
    Read = 0,
    Write = 1,
    Destructive = 2,
}

/// Staging confirms writes (same unlock as production). Type-to-confirm only
/// for DROP/TRUNCATE. Local has no extra prompt beyond read-only.
pub fn environment_needs_unlock(environment: &str) -> bool {
    matches!(
        environment.trim().to_ascii_lowercase().as_str(),
        "production" | "staging"
    )
}

pub fn environment_needs_type_confirm(environment: &str, risk: WriteRisk) -> bool {
    environment_needs_unlock(environment) && risk == WriteRisk::Destructive
}
