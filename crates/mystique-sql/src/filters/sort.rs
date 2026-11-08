//! Preview sort helpers shared by SQL and Mongo plugins.
//!
//! Columns are passed by name so the caller can resolve them against
//! whatever catalog type it holds.

use crate::{quote_ident, SqlDialect};

/// Resolve a requested sort column against described columns (exact name).
pub fn resolve_preview_sort_column<'a>(
    column_names: impl IntoIterator<Item = &'a str>,
    sort_column: Option<&str>,
) -> Option<&'a str> {
    let name = sort_column.map(str::trim).filter(|s| !s.is_empty())?;
    column_names.into_iter().find(|c| *c == name)
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
pub fn preview_sort_sql<'a>(
    dialect: SqlDialect,
    column_names: impl IntoIterator<Item = &'a str>,
    sort_column: Option<&str>,
    sort_dir: Option<&str>,
) -> Option<String> {
    let col = resolve_preview_sort_column(column_names, sort_column)?;
    let dir = if preview_sort_dir_desc(sort_dir) {
        "DESC"
    } else {
        "ASC"
    };
    Some(format!(" ORDER BY {} {}", quote_ident(dialect, col), dir))
}

/// Mongo `sort` field + direction (`1` / `-1`) when the column is known.
pub fn preview_sort_field<'a>(
    column_names: impl IntoIterator<Item = &'a str>,
    sort_column: Option<&str>,
    sort_dir: Option<&str>,
) -> Option<(&'a str, i32)> {
    let col = resolve_preview_sort_column(column_names, sort_column)?;
    let dir = if preview_sort_dir_desc(sort_dir) {
        -1
    } else {
        1
    };
    Some((col, dir))
}
