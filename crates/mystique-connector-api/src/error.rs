use serde::{Deserialize, Serialize};
use thiserror::Error;

mod pretty;
pub use pretty::prettify_sql_error;

pub type ConnectorResult<T> = Result<T, ConnectorError>;

/// Where a fault sits in the submitted batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePosition {
    /// 0-based byte offset.
    pub offset: u32,
    /// 1-based line.
    pub line: u32,
    /// 1-based column, counting characters.
    pub column: u32,
}

/// Line and column of `offset` in `text`. The offset snaps back to a char boundary.
pub fn position_at(text: &str, offset: u32) -> SourcePosition {
    let target = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .min(text.len());
    let mut line = 1u32;
    let mut column = 1u32;
    let mut index = 0usize;
    for ch in text.chars() {
        if index >= target {
            break;
        }
        let width = ch.len_utf8();
        if index + width > target {
            break;
        }
        index += width;
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    SourcePosition {
        offset: u32::try_from(index).unwrap_or(u32::MAX),
        line,
        column,
    }
}

/// IPC shape for a query command failure. Other commands stay plain strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryIpcError {
    pub kind: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<SourcePosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement_index: Option<u32>,
}

impl QueryIpcError {
    pub fn plain(message: impl Into<String>) -> Self {
        Self {
            kind: "internal".into(),
            message: message.into(),
            position: None,
            statement_index: None,
        }
    }
}

impl From<ConnectorError> for QueryIpcError {
    fn from(err: ConnectorError) -> Self {
        match &err {
            ConnectorError::QueryAt {
                message,
                position,
                statement_index,
            } => Self {
                kind: "query".into(),
                message: message.clone(),
                position: Some(position.clone()),
                statement_index: *statement_index,
            },
            _ => Self {
                kind: err.kind().into(),
                message: err.to_string(),
                position: None,
                statement_index: None,
            },
        }
    }
}

impl From<String> for QueryIpcError {
    fn from(message: String) -> Self {
        Self::plain(message)
    }
}

#[derive(Debug, Error)]
pub enum ConnectorError {
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("connection failed: {0}")]
    Connect(String),

    #[error("pool error: {0}")]
    Pool(String),

    #[error("timeout: {0}")]
    Timeout(String),

    #[error("not supported: {0}")]
    NotSupported(String),

    #[error("{0}")]
    Query(String),

    /// A query error that points at a character in the submitted text.
    ///
    /// `position.offset` is a 0-based byte offset. `line` and `column` are 1-based.
    #[error("{message}")]
    QueryAt {
        message: String,
        position: SourcePosition,
        statement_index: Option<u32>,
    },

    #[error("policy: {0}")]
    Policy(String),

    #[error("internal: {0}")]
    Internal(String),

    #[error("{0}")]
    SshHostKey(crate::SshHostKeyPrompt),

    #[error("{0}")]
    WriteGuard(crate::WriteGuardPrompt),
}

impl ConnectorError {
    pub fn query_at(
        message: impl Into<String>,
        position: SourcePosition,
        statement_index: Option<u32>,
    ) -> Self {
        Self::QueryAt {
            message: message.into(),
            position,
            statement_index,
        }
    }

    /// Byte offset relative to the statement the driver saw. None when the
    /// engine did not name a character.
    pub fn local_position(&self) -> Option<&SourcePosition> {
        match self {
            Self::QueryAt { position, .. } => Some(position),
            _ => None,
        }
    }

    pub fn user_message(&self) -> String {
        match self {
            Self::QueryAt { message, .. } => message.clone(),
            _ => self.to_string(),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidConfig(_) => "invalid_config",
            Self::Auth(_) => "auth",
            Self::Connect(_) => "connect",
            Self::Pool(_) => "pool",
            Self::Timeout(_) => "timeout",
            Self::NotSupported(_) => "not_supported",
            Self::Query(_) | Self::QueryAt { .. } => "query",
            Self::Policy(_) => "policy",
            Self::Internal(_) => "internal",
            Self::SshHostKey(_) => "ssh_host_key",
            Self::WriteGuard(_) => "write_guard",
        }
    }
}

impl Serialize for ConnectorError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let (message, position, statement_index) = match self {
            Self::QueryAt {
                message,
                position,
                statement_index,
            } => (message.clone(), Some(position.clone()), *statement_index),
            _ => (self.to_string(), None, None),
        };
        let mut s = serializer.serialize_struct("ConnectorError", 4)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &message)?;
        s.serialize_field("position", &position)?;
        s.serialize_field("statementIndex", &statement_index)?;
        s.end()
    }
}
