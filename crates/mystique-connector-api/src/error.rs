use serde::Serialize;
use thiserror::Error;

pub type ConnectorResult<T> = Result<T, ConnectorError>;

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

    #[error("policy: {0}")]
    Policy(String),

    #[error("internal: {0}")]
    Internal(String),
}

impl ConnectorError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidConfig(_) => "invalid_config",
            Self::Auth(_) => "auth",
            Self::Connect(_) => "connect",
            Self::Pool(_) => "pool",
            Self::Timeout(_) => "timeout",
            Self::NotSupported(_) => "not_supported",
            Self::Query(_) => "query",
            Self::Policy(_) => "policy",
            Self::Internal(_) => "internal",
        }
    }
}

impl Serialize for ConnectorError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("ConnectorError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}
