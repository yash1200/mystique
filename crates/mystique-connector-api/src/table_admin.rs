//! Catalog object admin SPI (rename / drop / truncate / duplicate / CREATE text).
//!
//! Plugins own the dialect and execution. The FE only sends the op + names.

use serde::{Deserialize, Serialize};

/// Explorer object kind (plugin may ignore and inspect the live catalog).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TableObjectKind {
    #[default]
    Table,
    View,
    Collection,
}

impl TableObjectKind {
    pub fn from_label(raw: Option<&str>) -> Self {
        match raw.unwrap_or("").trim().to_ascii_lowercase().as_str() {
            "view" => Self::View,
            "collection" => Self::Collection,
            _ => Self::Table,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TableAdminOp {
    /// Return engine-native CREATE / createCollection text. Does not execute.
    CreateStatement,
    Rename,
    Drop,
    Truncate,
    Duplicate,
}

/// IPC request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableAdminRequest {
    pub session_id: String,
    pub op: TableAdminOp,
    pub name: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    /// Required for rename + duplicate.
    #[serde(default)]
    pub new_name: Option<String>,
    #[serde(default)]
    pub profile_read_only: bool,
}

/// Validated payload handed to the plugin.
#[derive(Debug, Clone)]
pub struct TableAdminInput {
    pub op: TableAdminOp,
    pub name: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub kind: TableObjectKind,
    pub new_name: Option<String>,
}

impl TableAdminRequest {
    pub fn into_input(self) -> TableAdminInput {
        TableAdminInput {
            op: self.op,
            name: self.name,
            schema: self.schema,
            database: self.database,
            kind: TableObjectKind::from_label(self.kind.as_deref()),
            new_name: self.new_name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableAdminResult {
    pub op: TableAdminOp,
    /// Populated for [`TableAdminOp::CreateStatement`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statement: Option<String>,
    pub executed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl TableAdminResult {
    pub fn statement(op: TableAdminOp, statement: String) -> Self {
        Self {
            op,
            statement: Some(statement),
            executed: false,
            message: None,
        }
    }

    pub fn executed(op: TableAdminOp, message: impl Into<String>) -> Self {
        Self {
            op,
            statement: None,
            executed: true,
            message: Some(message.into()),
        }
    }
}
