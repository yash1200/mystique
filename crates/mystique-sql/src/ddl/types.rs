//! Structure drafts and plans. Serde shapes are part of the IPC contract.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ColumnOriginal {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub pk_position: u32,
    /// MySQL: the column definition after its name, as `SHOW CREATE TABLE`
    /// prints it. MODIFY is rebuilt from it so DEFAULT, ON UPDATE,
    /// AUTO_INCREMENT, COMMENT, charset, and collation survive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ColumnDraft {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub pk_position: u32,
    #[serde(default)]
    pub original: Option<ColumnOriginal>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub drop: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct IndexOriginal {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct IndexDraft {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub primary: bool,
    /// Postgres `CREATE INDEX CONCURRENTLY`. Cannot share a transaction.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub concurrent: bool,
    #[serde(default)]
    pub original: Option<IndexOriginal>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub drop: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyOriginal {
    pub name: String,
    pub columns: Vec<String>,
    #[serde(default)]
    pub ref_schema: Option<String>,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyDraft {
    pub name: String,
    pub columns: Vec<String>,
    #[serde(default)]
    pub ref_schema: Option<String>,
    pub ref_table: String,
    pub ref_columns: Vec<String>,
    #[serde(default)]
    pub original: Option<ForeignKeyOriginal>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub drop: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct TableDefinition {
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub schema: Option<String>,
    pub name: String,
    #[serde(default = "default_table_kind")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub kind: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub columns: Vec<ColumnDraft>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub indexes: Vec<IndexDraft>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub foreign_keys: Vec<ForeignKeyDraft>,
    /// SQLite `sqlite_master.sql` captured when the definition was read.
    #[serde(default)]
    pub schema_revision: Option<String>,
    #[serde(default)]
    pub validator: Option<String>,
    #[serde(default)]
    pub original_validator: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub create: bool,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub drop_table: bool,
    /// After an explicit rename pair, the table should end with this name.
    #[serde(default)]
    pub rename_to: Option<String>,
}

fn default_table_kind() -> String {
    "table".into()
}

impl ColumnDraft {
    /// A column just read from the catalog. `original` matches the live row.
    pub fn loaded(
        name: impl Into<String>,
        data_type: impl Into<String>,
        nullable: bool,
        pk_position: u32,
    ) -> Self {
        let name = name.into();
        let data_type = data_type.into();
        Self {
            name: name.clone(),
            data_type: data_type.clone(),
            nullable,
            pk_position,
            original: Some(ColumnOriginal {
                name,
                data_type,
                nullable,
                pk_position,
                definition: None,
            }),
            drop: false,
        }
    }
}

impl IndexDraft {
    pub fn loaded(
        name: impl Into<String>,
        columns: Vec<String>,
        unique: bool,
        primary: bool,
    ) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            columns: columns.clone(),
            unique,
            primary,
            concurrent: false,
            original: Some(IndexOriginal {
                name,
                columns,
                unique,
                primary,
            }),
            drop: false,
        }
    }
}

impl ForeignKeyDraft {
    pub fn loaded(
        name: impl Into<String>,
        columns: Vec<String>,
        ref_schema: Option<String>,
        ref_table: impl Into<String>,
        ref_columns: Vec<String>,
    ) -> Self {
        let name = name.into();
        let ref_table = ref_table.into();
        Self {
            name: name.clone(),
            columns: columns.clone(),
            ref_schema: ref_schema.clone(),
            ref_table: ref_table.clone(),
            ref_columns: ref_columns.clone(),
            original: Some(ForeignKeyOriginal {
                name,
                columns,
                ref_schema,
                ref_table,
                ref_columns,
            }),
            drop: false,
        }
    }
}

impl TableDefinition {
    pub fn loaded(
        database: Option<String>,
        schema: Option<String>,
        name: impl Into<String>,
        kind: &str,
    ) -> Self {
        Self {
            database,
            schema,
            name: name.into(),
            kind: kind.into(),
            columns: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            schema_revision: None,
            validator: None,
            original_validator: None,
            create: false,
            drop_table: false,
            rename_to: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct DdlPlan {
    pub statements: Vec<String>,
    pub warnings: Vec<String>,
    pub transactional: bool,
    pub requires_rebuild: bool,
    /// Shown with the preview. Not a warning, so it does not block apply.
    #[serde(default)]
    pub note: Option<String>,
}

impl DdlPlan {
    pub fn applyable(&self) -> bool {
        self.warnings.is_empty() && !self.statements.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct RenamePair {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SchemaChange {
    pub name: String,
    pub action: String,
    pub plan: DdlPlan,
    pub draft: TableDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SchemaMigration {
    pub changes: Vec<SchemaChange>,
    pub warnings: Vec<String>,
}
