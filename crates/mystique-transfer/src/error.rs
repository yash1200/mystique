/// Import or export failure. Variants mirror the SPI error kinds they map onto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferError {
    /// Unreadable file or a file the parser cannot use.
    InvalidConfig(String),
    /// The export file could not be written or encoded.
    Internal(String),
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(f, "invalid configuration: {message}"),
            Self::Internal(message) => write!(f, "internal: {message}"),
        }
    }
}

impl std::error::Error for TransferError {}

pub type TransferResult<T> = Result<T, TransferError>;
