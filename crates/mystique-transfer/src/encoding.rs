//! Text encodings an import file can be read as.

use serde::{Deserialize, Serialize};

use crate::error::{TransferError, TransferResult};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum ImportEncoding {
    #[default]
    Utf8,
    Utf16Le,
    Utf16Be,
    Windows1252,
    Iso88591,
}

impl ImportEncoding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Utf16Le => "utf-16le",
            Self::Utf16Be => "utf-16be",
            Self::Windows1252 => "windows-1252",
            Self::Iso88591 => "iso-8859-1",
        }
    }

    pub(crate) fn decoder(self) -> &'static encoding_rs::Encoding {
        match self {
            Self::Utf8 => encoding_rs::UTF_8,
            Self::Utf16Le => encoding_rs::UTF_16LE,
            Self::Utf16Be => encoding_rs::UTF_16BE,
            Self::Windows1252 | Self::Iso88591 => encoding_rs::WINDOWS_1252,
        }
    }
}

/// Read `path` and decode it. Invalid sequences become U+FFFD.
pub fn decode_file(path: &str, encoding: ImportEncoding) -> TransferResult<String> {
    let bytes = std::fs::read(path)
        .map_err(|e| TransferError::InvalidConfig(format!("could not read {path}: {e}")))?;
    let (cow, _, _) = encoding.decoder().decode(&bytes);
    Ok(cow.into_owned())
}
