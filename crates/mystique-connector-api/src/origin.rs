//! Where a query result column came from, and the key of that table.
//!
//! A clipped query cell gets a locator only when its source table and that
//! table's key are known and every key column is in the same result. The
//! driver says where a column came from (Postgres table oid and attribute
//! number, MySQL `org_table` / `org_name`). The plugin turns that into names
//! and the table key.

use serde::{Deserialize, Serialize};

/// Driver provenance for one result column. Never crosses IPC.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ColumnOrigin {
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table: Option<String>,
    pub column: Option<String>,
    /// Postgres RowDescription: table oid (0 for an expression).
    pub table_oid: Option<u32>,
    /// Postgres RowDescription: attribute number in that table.
    pub column_id: Option<i16>,
}

impl ColumnOrigin {
    /// A table column by name (MySQL `org_table` / `org_name`, a parsed SELECT).
    pub fn named(
        database: Option<String>,
        schema: Option<String>,
        table: impl Into<String>,
        column: impl Into<String>,
    ) -> Self {
        Self {
            database: nonempty(database),
            schema: nonempty(schema),
            table: Some(table.into()),
            column: Some(column.into()),
            table_oid: None,
            column_id: None,
        }
    }

    /// A Postgres column. `None` when the column is an expression.
    pub fn pg(table_oid: Option<u32>, column_id: Option<i16>) -> Option<Self> {
        match (table_oid, column_id) {
            (Some(oid), Some(attnum)) if oid != 0 && attnum > 0 => Some(Self {
                table_oid: Some(oid),
                column_id: Some(attnum),
                ..Self::default()
            }),
            _ => None,
        }
    }

    pub fn has_names(&self) -> bool {
        self.table.as_deref().is_some_and(|t| !t.trim().is_empty())
            && self.column.as_deref().is_some_and(|c| !c.trim().is_empty())
    }
}

/// A result column resolved to a table column. `table_key` is the table's
/// primary key (or, on Postgres, a unique key over NOT NULL columns). Empty
/// when the table has none, and then no locator is attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedColumn {
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table: String,
    pub column: String,
    pub data_type: Option<String>,
    pub table_key: Vec<KeyColumn>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyColumn {
    pub name: String,
    pub data_type: Option<String>,
}

impl ResolvedColumn {
    /// Same source table, compared by name.
    pub fn same_table(&self, other: &ResolvedColumn) -> bool {
        self.table == other.table && self.schema == other.schema && self.database == other.database
    }
}

fn nonempty(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.trim().is_empty())
}
