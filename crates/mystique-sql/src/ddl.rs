//! Structure edits and same-engine schema diff.
//!
//! The editor sends a draft. This module is the only place that turns that
//! draft into SQL or Mongo commands. A warning means the plan cannot be applied.
//!
//! - [`types`]: the draft and plan shapes (IPC contract; the SPI re-exports them).
//! - `validate` / `changes`: draft checks and "what changed" predicates.
//! - `statements`: quoting and the CREATE / INDEX / FOREIGN KEY / RENAME builders.
//! - `alter` with `pg` and `mysql`: in-place ALTER for Postgres and MySQL.
//! - `sqlite`, `mongo`: the engines with their own rules.
//! - `diff` and `order`: schema diff between two table lists, FK-safe order.

mod alter;
mod changes;
mod diff;
mod mongo;
mod mysql;
mod order;
mod pg;
mod sqlite;
mod sqlite_text;
mod statements;
pub mod types;
mod validate;

#[cfg(test)]
mod diff_tests;
#[cfg(test)]
mod sqlite_tests;
#[cfg(test)]
mod tests;

pub use diff::{plan_schema_diff, plan_schema_diff_with};
pub use mysql::mysql_column_default;
pub use sqlite::steps as sqlite_rebuild_steps;
pub use sqlite_text::{sqlite_table_extras, SqliteForeignKey, SqliteTableExtras};
pub use types::{
    clean_action, ColumnDraft, ColumnOriginal, ColumnRename, ConstraintDraft, ConstraintOriginal,
    DdlPlan, DiffSelection, DropItem, DropKey, ForeignKeyDraft, ForeignKeyOriginal, IndexDraft,
    IndexOriginal, RenamePair, SchemaChange, SchemaMigration, TableDefinition, TriggerDefinition,
};

use crate::SqlDialect;
use changes::has_structural_edit;
use validate::{validate_columns, validate_constraints, validate_foreign_keys, validate_indexes};

/// What a structure plan targets: a SQL dialect, or MongoDB (indexes and
/// validators only). The one SQL dialect enum is [`SqlDialect`]; a generic
/// SQL engine is planned like Postgres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DdlTarget {
    Sql(SqlDialect),
    Mongo,
}

impl DdlTarget {
    pub const POSTGRES: Self = Self::Sql(SqlDialect::Postgres);
    pub const MYSQL: Self = Self::Sql(SqlDialect::Mysql);
    pub const SQLITE: Self = Self::Sql(SqlDialect::Sqlite);

    pub fn from_engine(engine: &str, sql_dialect: &str) -> Self {
        let engine = engine.trim().to_ascii_lowercase();
        if engine == "mongodb" || engine == "documentdb" {
            return Self::Mongo;
        }
        match sql_dialect.trim().to_ascii_lowercase().as_str() {
            "mysql" => Self::MYSQL,
            "sqlite" => Self::SQLITE,
            "postgresql" | "postgres" => Self::POSTGRES,
            _ => match engine.as_str() {
                "mysql" | "mariadb" | "tidb" => Self::MYSQL,
                "sqlite" => Self::SQLITE,
                _ => Self::POSTGRES,
            },
        }
    }

    /// Dialect used for quoting. Mongo plans hold no SQL; Postgres is a stand-in.
    pub fn sql(self) -> SqlDialect {
        match self {
            Self::Sql(dialect) => dialect,
            Self::Mongo => SqlDialect::Postgres,
        }
    }

    fn is_postgres(self) -> bool {
        matches!(self, Self::Sql(SqlDialect::Postgres | SqlDialect::Generic))
    }

    fn is_mysql(self) -> bool {
        self == Self::MYSQL
    }

    fn is_sqlite(self) -> bool {
        self == Self::SQLITE
    }
}

impl From<SqlDialect> for DdlTarget {
    fn from(dialect: SqlDialect) -> Self {
        Self::Sql(dialect)
    }
}

pub fn plan_table_alter(dialect: impl Into<DdlTarget>, draft: &TableDefinition) -> DdlPlan {
    let dialect = dialect.into();
    let mut warnings = Vec::new();
    if draft.create && draft.drop_table {
        warnings.push("A table cannot be created and dropped in one change.".into());
    }
    if draft.name.trim().is_empty() {
        warnings.push("Table name is empty.".into());
    }
    if !draft.drop_table
        && !draft.create
        && draft.kind.eq_ignore_ascii_case("view")
        && has_structural_edit(draft)
    {
        warnings.push("Views cannot be edited here.".into());
    }
    validate_columns(draft, dialect.is_sqlite(), &mut warnings);
    validate_indexes(draft, &mut warnings);
    validate_foreign_keys(draft, &mut warnings);
    validate_constraints(draft, &mut warnings);

    if dialect == DdlTarget::Mongo {
        return mongo::plan_mongo(draft, warnings);
    }
    if draft.drop_table {
        return finish(
            dialect,
            vec![format!(
                "DROP TABLE {}",
                statements::qualify(dialect, draft)
            )],
            warnings,
            false,
            false,
            None,
        );
    }
    if draft.create {
        return plan_create(dialect, draft, warnings);
    }
    if !warnings.is_empty() {
        return finish(dialect, Vec::new(), warnings, false, false, None);
    }
    if dialect.is_sqlite() {
        sqlite::plan_sqlite(draft)
    } else {
        alter::plan_sql(dialect, draft, dialect.is_postgres())
    }
}

fn plan_create(dialect: DdlTarget, draft: &TableDefinition, mut warnings: Vec<String>) -> DdlPlan {
    if dialect == DdlTarget::Mongo {
        warnings.push("Creating a Mongo collection from a column draft is not supported.".into());
        return finish(dialect, Vec::new(), warnings, false, false, None);
    }
    let mut statements = vec![statements::create_table_sql(dialect, draft)];
    statements.extend(statements::index_statements(dialect, draft, false));
    if !dialect.is_sqlite() {
        statements.extend(statements::foreign_key_statements(dialect, draft).1);
    }
    let transactional = dialect.is_postgres();
    finish(dialect, statements, warnings, transactional, false, None)
}

pub(super) fn finish(
    dialect: DdlTarget,
    statements: Vec<String>,
    warnings: Vec<String>,
    transactional: bool,
    requires_rebuild: bool,
    note: Option<String>,
) -> DdlPlan {
    let _ = dialect;
    let blocked = !warnings.is_empty();
    DdlPlan {
        statements: if blocked { Vec::new() } else { statements },
        transactional: transactional && !blocked,
        requires_rebuild: requires_rebuild && !blocked,
        warnings,
        note,
    }
}
