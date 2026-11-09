//! shim: remove after one release. Grid-edit SQL lives in `mystique_sql::dml`,
//! quoting and literals in `mystique_sql`. These wrappers keep the old
//! `ConnectorResult` signatures; new code calls `mystique_sql` directly.

use mystique_sql::dml;

use super::edits::{
    ApplyTableEditsInput, DistinctColumnValuesInput, TableRowDelete, TableRowInsert, TableRowUpdate,
};
use crate::catalog::ColumnInfo;
use crate::error::{ConnectorError, ConnectorResult};
use crate::large_cell::SqlPlaceholder;

pub use mystique_sql::dml::{EditPlan, PlannedCount, PlannedEdit, PlannedRead};
pub use mystique_sql::{qualify_table, quote_ident, sql_literal};

/// shim: remove after one release. The one dialect enum is [`mystique_sql::SqlDialect`];
/// SQLite callers that passed `Postgres` get identical output from `Sqlite`.
pub type SqlIdentStyle = mystique_sql::SqlDialect;

pub fn compile_update(
    style: SqlIdentStyle,
    input: &ApplyTableEditsInput,
    update: &TableRowUpdate,
) -> ConnectorResult<String> {
    Ok(dml::compile_update(style, input, update)?)
}

pub fn compile_insert(
    style: SqlIdentStyle,
    input: &ApplyTableEditsInput,
    insert: &TableRowInsert,
) -> ConnectorResult<String> {
    Ok(dml::compile_insert(style, input, insert)?)
}

pub fn compile_upsert(
    style: SqlIdentStyle,
    input: &ApplyTableEditsInput,
    insert: &TableRowInsert,
) -> ConnectorResult<String> {
    Ok(dml::compile_upsert(style, input, insert)?)
}

pub fn compile_delete(
    style: SqlIdentStyle,
    input: &ApplyTableEditsInput,
    delete: &TableRowDelete,
) -> ConnectorResult<String> {
    Ok(dml::compile_delete(style, input, delete)?)
}

pub fn plan_table_edits(
    style: SqlIdentStyle,
    placeholder: SqlPlaceholder,
    input: &ApplyTableEditsInput,
) -> ConnectorResult<EditPlan> {
    Ok(dml::plan_table_edits(style, placeholder, input)?)
}

/// `SELECT DISTINCT` for the grid filter. The column must be one the catalog returned.
pub fn distinct_values_sql(
    style: SqlIdentStyle,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    column: &str,
    known: &[ColumnInfo],
    limit: u32,
) -> ConnectorResult<String> {
    if !known.iter().any(|info| info.name == column) {
        return Err(ConnectorError::Query(format!("Unknown column “{column}”.")));
    }
    Ok(dml::distinct_values_sql(
        style, database, schema, table, column, limit, None,
    ))
}

/// [`distinct_values_sql`] for one plugin request, prefix included.
pub fn distinct_input_sql(
    style: SqlIdentStyle,
    input: &DistinctColumnValuesInput,
    known: &[ColumnInfo],
) -> ConnectorResult<String> {
    let column = input.column.as_str();
    if !known.iter().any(|info| info.name == column) {
        return Err(ConnectorError::Query(format!("Unknown column “{column}”.")));
    }
    Ok(dml::distinct_values_sql(
        style,
        input.database.as_deref(),
        input.schema.as_deref(),
        &input.table,
        column,
        input.limit,
        input.prefix.as_deref(),
    ))
}

#[cfg(test)]
mod distinct_tests {
    use super::distinct_values_sql;
    use super::SqlIdentStyle;
    use crate::ColumnInfo;

    fn column(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
            pk: false,
            enum_values: None,
            default_value: None,
            references: None,
        }
    }

    #[test]
    fn distinct_quotes_a_known_column_and_caps_the_limit() {
        let sql = distinct_values_sql(
            SqlIdentStyle::Postgres,
            None,
            Some("public"),
            "docs",
            "body",
            &[column("id"), column("body")],
            5_000,
        )
        .expect("sql");
        assert_eq!(
            sql,
            "SELECT DISTINCT \"body\" FROM \"public\".\"docs\" WHERE \"body\" IS NOT NULL LIMIT 1000"
        );
        assert!(distinct_values_sql(
            SqlIdentStyle::Mysql,
            Some("app"),
            None,
            "docs",
            "nope",
            &[column("id")],
            10,
        )
        .is_err());
    }
}
