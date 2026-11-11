//! Catalog object admin SPI (rename / drop / truncate / duplicate / CREATE text).
//!
//! Plugins own the dialect and execution. The FE only sends the op + names.

use serde::{Deserialize, Serialize};

/// Explorer object kind (plugin may ignore and inspect the live catalog).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum TableAdminOp {
    /// Return engine-native CREATE / createCollection text. Does not execute.
    CreateStatement,
    /// Create a group/namespace (e.g. Postgres `CREATE SCHEMA`). `name` is the schema.
    CreateGroup,
    /// Create a leaf object (table / collection). `name` + optional `schema` / `database`.
    CreateObject,
    Rename,
    Drop,
    Truncate,
    Duplicate,
}

/// IPC request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
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
    pub confirm_object_name: Option<String>,
}

/// Validated payload handed to the plugin.
#[derive(Debug, Clone)]
pub struct TableAdminInput {
    pub op: TableAdminOp,
    pub name: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub kind: TableObjectKind,
    /// The catalog kind as listed (`table`, `matview`, `function`, …),
    /// lower case. [`TableAdminInput::kind`] folds every open kind to `Table`;
    /// plugins that list open kinds read this one.
    pub object_kind: String,
    pub new_name: Option<String>,
}

/// Kinds every admin op applies to. Others (functions, sequences, types, …)
/// only offer [`TableAdminOp::CreateStatement`] until a plugin says more.
pub const ADMIN_KINDS: &[&str] = &["table", "view", "collection"];

/// Lower-cased kind label; empty means `table`.
pub fn normalize_object_kind(raw: Option<&str>) -> String {
    let k = raw.unwrap_or("").trim().to_ascii_lowercase();
    if k.is_empty() {
        "table".into()
    } else {
        k
    }
}

/// Whether `op` may run on an object of `kind`. Open kinds are read-only
/// (their definition only); mutating them goes through the query editor.
pub fn admin_op_allowed(kind: &str, op: TableAdminOp) -> bool {
    matches!(
        op,
        TableAdminOp::CreateStatement | TableAdminOp::CreateGroup | TableAdminOp::CreateObject
    ) || ADMIN_KINDS.contains(&kind)
}

impl TableAdminRequest {
    pub fn into_input(self) -> TableAdminInput {
        TableAdminInput {
            op: self.op,
            name: self.name,
            schema: self.schema,
            database: self.database,
            kind: TableObjectKind::from_label(self.kind.as_deref()),
            object_kind: normalize_object_kind(self.kind.as_deref()),
            new_name: self.new_name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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

#[cfg(test)]
mod kind_tests {
    use super::*;

    #[test]
    fn open_kinds_only_offer_their_definition() {
        for op in [
            TableAdminOp::Rename,
            TableAdminOp::Drop,
            TableAdminOp::Truncate,
            TableAdminOp::Duplicate,
        ] {
            assert!(!admin_op_allowed("function", op), "{op:?} on a function");
            assert!(admin_op_allowed("table", op));
            assert!(admin_op_allowed("view", op));
            assert!(admin_op_allowed("collection", op));
        }
        assert!(admin_op_allowed("function", TableAdminOp::CreateStatement));
        assert!(admin_op_allowed("sequence", TableAdminOp::CreateStatement));
    }

    #[test]
    fn kind_label_is_lower_case_and_defaults_to_table() {
        assert_eq!(normalize_object_kind(None), "table");
        assert_eq!(normalize_object_kind(Some("  ")), "table");
        assert_eq!(normalize_object_kind(Some("MatView")), "matview");
        let input = TableAdminRequest {
            session_id: "s".into(),
            op: TableAdminOp::CreateStatement,
            name: "f(integer)".into(),
            schema: None,
            database: None,
            kind: Some("Function".into()),
            new_name: None,
            confirm_object_name: None,
        }
        .into_input();
        assert_eq!(input.kind, TableObjectKind::Table);
        assert_eq!(input.object_kind, "function");
    }
}
