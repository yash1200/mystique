//! Preview sort helpers shared by SQL and Mongo plugins.

use super::super::sql::{quote_ident, SqlIdentStyle};
use crate::catalog::ColumnInfo;

/// Resolve a requested sort column against described columns (exact name).
pub fn resolve_preview_sort_column<'a>(
    columns: &'a [ColumnInfo],
    sort_column: Option<&str>,
) -> Option<&'a str> {
    let name = sort_column.map(str::trim).filter(|s| !s.is_empty())?;
    columns
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.name.as_str())
}

fn preview_sort_dir_desc(sort_dir: Option<&str>) -> bool {
    matches!(
        sort_dir
            .unwrap_or("asc")
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "desc" | "descending" | "-1"
    )
}

/// ` ORDER BY "col" ASC` when the column is known; `None` = plugin default order.
pub fn preview_sort_sql(
    style: SqlIdentStyle,
    columns: &[ColumnInfo],
    sort_column: Option<&str>,
    sort_dir: Option<&str>,
) -> Option<String> {
    let col = resolve_preview_sort_column(columns, sort_column)?;
    let dir = if preview_sort_dir_desc(sort_dir) {
        "DESC"
    } else {
        "ASC"
    };
    Some(format!(" ORDER BY {} {}", quote_ident(style, col), dir))
}

/// Mongo `sort` field + direction (`1` / `-1`) when the column is known.
pub fn preview_sort_field<'a>(
    columns: &'a [ColumnInfo],
    sort_column: Option<&str>,
    sort_dir: Option<&str>,
) -> Option<(&'a str, i32)> {
    let col = resolve_preview_sort_column(columns, sort_column)?;
    let dir = if preview_sort_dir_desc(sort_dir) {
        -1
    } else {
        1
    };
    Some((col, dir))
}
