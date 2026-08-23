//! Integer primary keys stored in SQLite, exposed as decimal strings in the API.

use rusqlite::Row;

use crate::error::{AppError, AppResult};

pub fn parse_id(id: &str) -> AppResult<i64> {
    id.trim()
        .parse::<i64>()
        .map_err(|_| AppError::Invalid(format!("id must be a number, got '{id}'")))
}

pub fn parse_id_opt(id: Option<&str>) -> AppResult<Option<i64>> {
    match id.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => Ok(Some(parse_id(s)?)),
    }
}

pub fn id_string(row: &Row<'_>, col: &str) -> rusqlite::Result<String> {
    Ok(row.get::<_, i64>(col)?.to_string())
}

pub fn id_string_opt(row: &Row<'_>, col: &str) -> rusqlite::Result<Option<String>> {
    let v: Option<i64> = row.get(col)?;
    Ok(v.map(|n| n.to_string()))
}
