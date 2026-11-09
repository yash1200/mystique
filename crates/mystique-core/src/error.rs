//! Shared application errors (storage + services + connectors).

use mystique_connector_api::{ConnectorError, IpcError, IpcErrorKind, IpcPrompt};
use serde::Serialize;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    Invalid(String),

    #[error("connector error: {0}")]
    Connector(#[from] ConnectorError),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("{0}")]
    SecretStore(String),

    /// The profile does not save its password and none is held this session.
    /// The IPC envelope turns this into `kind: password_required` with a
    /// `PasswordRequired` prompt; the prefix stays in `Display` for the agent.
    #[error("password_required: This connection asks for its password once per session.")]
    PasswordRequired,

    /// An agent or MCP call above its access level (F-02). The text is for
    /// the model: what is needed and how the user can grant it.
    #[error("{0}")]
    AccessDenied(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<AppError> for String {
    fn from(value: AppError) -> Self {
        value.to_string()
    }
}

/// Text after the machine prefix of [`AppError::PasswordRequired`].
const PASSWORD_REQUIRED_MESSAGE: &str = "This connection asks for its password once per session.";

/// The IPC envelope. `Display` keeps its prefixes for MCP and the agent;
/// the envelope moves them into `kind` and `prompt`.
impl From<AppError> for IpcError {
    fn from(value: AppError) -> Self {
        let kind = match value {
            AppError::Connector(inner) => return IpcError::from(inner),
            AppError::PasswordRequired => {
                return IpcError {
                    prompt: Some(Box::new(IpcPrompt::PasswordRequired)),
                    ..IpcError::new(IpcErrorKind::PasswordRequired, PASSWORD_REQUIRED_MESSAGE)
                };
            }
            AppError::Database(_) | AppError::Io(_) | AppError::Json(_) => IpcErrorKind::Storage,
            AppError::NotFound(_) => IpcErrorKind::NotFound,
            AppError::Invalid(_) => IpcErrorKind::InvalidInput,
            AppError::Internal(_) => IpcErrorKind::Internal,
            AppError::SecretStore(_) => IpcErrorKind::SecretStore,
            AppError::AccessDenied(_) => IpcErrorKind::Policy,
        };
        IpcError::new(kind, value.to_string())
    }
}

#[cfg(test)]
mod ipc_tests {
    use super::*;

    #[test]
    fn password_required_is_a_typed_prompt_without_the_prefix() {
        let err = AppError::PasswordRequired;
        assert!(err.to_string().starts_with("password_required"));
        let value = serde_json::to_value(IpcError::from(err)).expect("serialize");
        assert_eq!(value["kind"], "password_required");
        assert_eq!(value["prompt"]["type"], "passwordRequired");
        assert_eq!(value["message"], PASSWORD_REQUIRED_MESSAGE);
    }

    #[test]
    fn a_connector_error_drops_the_wrapper() {
        let err = AppError::Connector(ConnectorError::Policy("Read-only.".into()));
        let value = serde_json::to_value(IpcError::from(err)).expect("serialize");
        assert_eq!(value["kind"], "policy");
        assert_eq!(value["message"], "Read-only.");
    }

    #[test]
    fn app_errors_keep_their_text() {
        let value = serde_json::to_value(IpcError::from(AppError::NotFound("profile x".into())))
            .expect("serialize");
        assert_eq!(value["kind"], "not_found");
        assert_eq!(value["message"], "not found: profile x");
    }
}
