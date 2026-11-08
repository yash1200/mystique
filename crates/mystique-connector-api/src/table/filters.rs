//! Grid filter compilation lives in `mystique_sql::filters`.
//!
//! The `ConnectorResult` wrappers (`sql_where_clause`, `sql_preview_where`,
//! `parse_code_filter_rules`, `mongo_filter_json`) are a shim: remove after
//! one release. The sort helpers stay: they resolve against the SPI's
//! `ColumnInfo`.

use mystique_sql::{filters, SqlDialect};

use crate::catalog::{ColumnInfo, TableFilter};
use crate::error::ConnectorResult;

/// ` WHERE a AND b` or empty. Identifiers are quoted; strings escaped.
pub fn sql_where_clause(style: SqlDialect, filters: &[TableFilter]) -> ConnectorResult<String> {
    Ok(filters::sql_where_clause(style, filters)?)
}

/// Compile UI rules plus the raw code-filter box into ` WHERE …`.
pub fn sql_preview_where(
    style: SqlDialect,
    filters: &[TableFilter],
    code_filter: Option<&str>,
) -> ConnectorResult<String> {
    Ok(filters::sql_preview_where(style, filters, code_filter)?)
}

/// Parse the code-filter box: JSON `{ "col": "val" }` or `col = val AND …`.
pub fn parse_code_filter_rules(code: &str) -> ConnectorResult<Vec<TableFilter>> {
    Ok(filters::parse_code_filter_rules(code)?)
}

/// JSON object the Mongo plugin turns into a BSON filter (`$and` when needed).
pub fn mongo_filter_json(filters: &[TableFilter]) -> ConnectorResult<serde_json::Value> {
    Ok(filters::mongo_filter_json(filters)?)
}

/// ` ORDER BY "col" ASC` when the column is known; `None` = plugin default order.
pub fn preview_sort_sql(
    style: SqlDialect,
    columns: &[ColumnInfo],
    sort_column: Option<&str>,
    sort_dir: Option<&str>,
) -> Option<String> {
    filters::preview_sort_sql(style, column_names(columns), sort_column, sort_dir)
}

/// Mongo `sort` field + direction (`1` / `-1`) when the column is known.
pub fn preview_sort_field<'a>(
    columns: &'a [ColumnInfo],
    sort_column: Option<&str>,
    sort_dir: Option<&str>,
) -> Option<(&'a str, i32)> {
    filters::preview_sort_field(column_names(columns), sort_column, sort_dir)
}

fn column_names(columns: &[ColumnInfo]) -> impl Iterator<Item = &str> {
    columns.iter().map(|c| c.name.as_str())
}
