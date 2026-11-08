//! Structure edits: IPC requests and write-risk for DDL plans.
//!
//! The planner lives in `mystique_sql::ddl`; its types and functions are
//! re-exported here so plugins and core keep their imports.

use serde::{Deserialize, Serialize};

pub use mystique_sql::ddl::{
    plan_schema_diff, plan_table_alter, ColumnDraft, ColumnOriginal, DdlPlan, DdlTarget,
    ForeignKeyDraft, ForeignKeyOriginal, IndexDraft, IndexOriginal, RenamePair, SchemaChange,
    SchemaMigration, TableDefinition,
};

use crate::WriteRisk;

/// shim: remove after one release. Use [`DdlTarget`]: `DdlTarget::Sql(SqlDialect)`
/// (or `DdlTarget::POSTGRES` / `MYSQL` / `SQLITE`) and `DdlTarget::Mongo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DdlDialect {
    Postgres,
    Mysql,
    Sqlite,
    Mongo,
}

impl DdlDialect {
    pub fn from_engine(engine: &str, sql_dialect: &str) -> Self {
        match DdlTarget::from_engine(engine, sql_dialect) {
            DdlTarget::Mongo => Self::Mongo,
            DdlTarget::MYSQL => Self::Mysql,
            DdlTarget::SQLITE => Self::Sqlite,
            DdlTarget::Sql(_) => Self::Postgres,
        }
    }
}

impl From<DdlDialect> for DdlTarget {
    fn from(dialect: DdlDialect) -> Self {
        match dialect {
            DdlDialect::Postgres => DdlTarget::POSTGRES,
            DdlDialect::Mysql => DdlTarget::MYSQL,
            DdlDialect::Sqlite => DdlTarget::SQLITE,
            DdlDialect::Mongo => DdlTarget::Mongo,
        }
    }
}

pub fn ddl_risk(draft: &TableDefinition) -> WriteRisk {
    if draft.drop_table || draft.columns.iter().any(|column| column.drop) {
        WriteRisk::Destructive
    } else {
        WriteRisk::Write
    }
}

pub fn migration_risk(changes: &[SchemaChange]) -> WriteRisk {
    if changes.iter().any(|change| {
        change.action == "drop"
            || change.plan.statements.iter().any(|sql| {
                sql.to_ascii_uppercase().contains("DROP COLUMN") || sql.starts_with("DROP TABLE")
            })
    }) {
        WriteRisk::Destructive
    } else {
        WriteRisk::Write
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyDdlResult {
    pub applied: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribeTableDefinitionRequest {
    pub session_id: String,
    pub table: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListTableDefinitionsRequest {
    pub session_id: String,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub schema: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanDdlRequest {
    pub session_id: String,
    pub draft: TableDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyDdlRequest {
    pub session_id: String,
    pub draft: TableDefinition,
    #[serde(default)]
    pub profile_read_only: bool,
    #[serde(default)]
    pub confirm_object_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSchemaDiffRequest {
    pub session_id: String,
    pub source: Vec<TableDefinition>,
    pub target: Vec<TableDefinition>,
    #[serde(default)]
    pub renames: Vec<RenamePair>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplySchemaDiffRequest {
    pub session_id: String,
    pub source: Vec<TableDefinition>,
    pub target: Vec<TableDefinition>,
    #[serde(default)]
    pub renames: Vec<RenamePair>,
    #[serde(default)]
    pub profile_read_only: bool,
    #[serde(default)]
    pub confirm_object_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialect_shim_maps_onto_the_same_target() {
        for (engine, dialect) in [
            ("postgres", ""),
            ("mysql", "mysql"),
            ("mariadb", ""),
            ("sqlite", "sqlite"),
            ("mongodb", ""),
            ("documentdb", "postgres"),
            ("duckdb", ""),
        ] {
            assert_eq!(
                DdlTarget::from(DdlDialect::from_engine(engine, dialect)),
                DdlTarget::from_engine(engine, dialect),
                "{engine}"
            );
        }
    }
}
