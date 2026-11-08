//! Large cell values cross IPC as a short preview. The full text is fetched
//! later by primary key, one chunk at a time.
//!
//! Agent and MCP keep their own character window. This clip runs only on the
//! grid IPC path, after the plugin has returned the real cell. Table export
//! reads the plugin preview directly and must not call [`clip_table_preview`].

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};

use crate::cancel::QueryControl;
use crate::error::{ConnectorError, ConnectorResult};
use crate::query::QueryResult;
use crate::{QueryBatchResult, TablePreviewResult};
use mystique_sql::{qualify_table, quote_ident, SqlDialect};

pub(crate) use mystique_sql::dml::is_large_marker;
pub use mystique_sql::SqlPlaceholder;

/// Encoded JSON size above which a grid cell is replaced with a preview.
pub const LARGE_CELL_BYTES: usize = 32 * 1024;
/// Same display clip the grid already uses (`CELL_TEXT_MAX`).
pub const LARGE_CELL_PREVIEW_CHARS: usize = 512;
/// Characters returned by one [`crate::FetchLargeValueResult`].
pub const LARGE_CELL_CHUNK_CHARS: u64 = 32_000;

const ELLIPSIS: char = '\u{2026}';

/// How a SQL engine spells a text cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlTextCast {
    /// `CAST(col AS text)` — Postgres and SQLite.
    Text,
    /// `CAST(col AS CHAR)` — MySQL.
    Char,
}

/// One primary-key value, already in the js-safe JSON form the grid uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeCellKey {
    pub column: String,
    pub value: JsonValue,
}

/// Handed back by the grid to load the rest of one cell. No SQL text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeCellLocator {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub table: String,
    pub column: String,
    pub keys: Vec<LargeCellKey>,
}

/// `SELECT` plus the text parameters. Values are never interpolated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeCellSelect {
    pub sql: String,
    pub params: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchLargeValueRequest {
    pub session_id: String,
    pub locator: LargeCellLocator,
    #[serde(default)]
    pub offset: u64,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub execution_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FetchLargeValueInput {
    pub locator: LargeCellLocator,
    pub offset: u64,
    pub limit: u64,
    pub control: Arc<QueryControl>,
}

impl FetchLargeValueRequest {
    pub fn into_input(self) -> FetchLargeValueInput {
        let limit = self.limit.unwrap_or(LARGE_CELL_CHUNK_CHARS);
        FetchLargeValueInput {
            locator: self.locator,
            offset: self.offset,
            limit: limit.clamp(1, LARGE_CELL_CHUNK_CHARS),
            control: QueryControl::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchLargeValueResult {
    pub text: String,
    pub offset: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<u64>,
    pub total_chars: u64,
}

struct ClipCol {
    name: String,
    pk: bool,
}

/// Build a single-cell read. An empty key list is refused: there is no
/// "match every column" fallback.
pub fn large_cell_select(
    style: SqlDialect,
    placeholder: SqlPlaceholder,
    cast: SqlTextCast,
    locator: &LargeCellLocator,
) -> ConnectorResult<LargeCellSelect> {
    if locator.table.trim().is_empty() || locator.column.trim().is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "A large value needs a table and a column.".into(),
        ));
    }
    if locator.keys.is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "This value cannot be fetched without a primary key.".into(),
        ));
    }

    let mut params = Vec::new();
    let mut parts = Vec::with_capacity(locator.keys.len());
    for key in &locator.keys {
        if key.column.trim().is_empty() {
            return Err(ConnectorError::InvalidConfig(
                "A primary key column name is required.".into(),
            ));
        }
        let ident = quote_ident(style, &key.column);
        match key_text(&key.value) {
            None => parts.push(format!("{ident} IS NULL")),
            Some(text) => {
                params.push(text);
                let slot = match placeholder {
                    SqlPlaceholder::Dollar => format!("${}", params.len()),
                    SqlPlaceholder::Question => "?".to_string(),
                };
                let cast_expr = match cast {
                    SqlTextCast::Text => format!("CAST({ident} AS text)"),
                    SqlTextCast::Char => format!("CAST({ident} AS CHAR)"),
                };
                parts.push(format!("{cast_expr} = {slot}"));
            }
        }
    }

    let col = quote_ident(style, &locator.column);
    let from = qualify_table(
        style,
        locator.database.as_deref(),
        locator.schema.as_deref(),
        &locator.table,
    );
    let where_sql = parts.join(" AND ");
    Ok(LargeCellSelect {
        sql: format!("SELECT {col} FROM {from} WHERE {where_sql} LIMIT 1"),
        params,
    })
}

/// Slice the display text of one freshly read cell.
pub fn page_cell(value: &JsonValue, offset: u64, limit: u64) -> FetchLargeValueResult {
    let text = cell_display_text(value);
    let total = text.chars().count() as u64;
    let start = offset.min(total);
    let take = limit.clamp(1, LARGE_CELL_CHUNK_CHARS);
    let end = start.saturating_add(take).min(total);
    let slice: String = text
        .chars()
        .skip(start as usize)
        .take((end - start) as usize)
        .collect();
    let next_offset = if end < total { Some(end) } else { None };
    FetchLargeValueResult {
        text: slice,
        offset: start,
        next_offset,
        total_chars: total,
    }
}

/// Replace oversized cells in a table page. Primary-key values stay on the
/// locator only when every key itself fits in the IPC budget.
pub fn clip_table_preview(
    result: &mut TablePreviewResult,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) {
    let cols: Vec<ClipCol> = result
        .columns
        .iter()
        .map(|c| ClipCol {
            name: c.name.clone(),
            pk: c.pk,
        })
        .collect();
    clip_rows(&cols, &mut result.rows, database, schema, table);
}

/// Query results have no primary-key metadata, so they preview without a locator.
pub fn clip_query_result(result: &mut QueryResult) {
    let cols: Vec<ClipCol> = result
        .columns
        .iter()
        .map(|c| ClipCol {
            name: c.name.clone(),
            pk: false,
        })
        .collect();
    clip_rows(&cols, &mut result.rows, None, None, "");
}

pub fn clip_batch_result(result: &mut QueryBatchResult) {
    for stmt in &mut result.results {
        if let Some(query) = stmt.result.as_mut() {
            clip_query_result(query);
        }
    }
}

fn clip_rows(
    cols: &[ClipCol],
    rows: &mut [Vec<JsonValue>],
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
) {
    for row in rows.iter_mut() {
        let keys = pk_keys(cols, row);
        for (index, cell) in row.iter_mut().enumerate() {
            if is_large_marker(cell) || encoded_len(cell) <= LARGE_CELL_BYTES {
                continue;
            }
            let column = cols.get(index).map(|c| c.name.clone()).unwrap_or_default();
            let locator = keys.as_ref().and_then(|keys| {
                if column.is_empty() || table.trim().is_empty() {
                    return None;
                }
                Some(LargeCellLocator {
                    database: nonempty(database),
                    schema: nonempty(schema),
                    table: table.to_string(),
                    column,
                    keys: keys.clone(),
                })
            });
            *cell = marker(cell, locator);
        }
    }
}

fn pk_keys(cols: &[ClipCol], row: &[JsonValue]) -> Option<Vec<LargeCellKey>> {
    let mut keys = Vec::new();
    let mut saw_pk = false;
    for (index, col) in cols.iter().enumerate() {
        if !col.pk {
            continue;
        }
        saw_pk = true;
        let value = row.get(index).cloned().unwrap_or(JsonValue::Null);
        if encoded_len(&value) > LARGE_CELL_BYTES {
            return None;
        }
        keys.push(LargeCellKey {
            column: col.name.clone(),
            value,
        });
    }
    if saw_pk {
        Some(keys)
    } else {
        None
    }
}

fn marker(value: &JsonValue, locator: Option<LargeCellLocator>) -> JsonValue {
    let full = cell_display_text(value);
    let total = full.chars().count() as u64;
    let mut preview: String = full.chars().take(LARGE_CELL_PREVIEW_CHARS).collect();
    if total as usize > LARGE_CELL_PREVIEW_CHARS {
        preview.push(ELLIPSIS);
    }
    let mut map = Map::new();
    map.insert("$large".to_string(), JsonValue::Bool(true));
    map.insert("preview".to_string(), JsonValue::String(preview));
    map.insert("totalChars".to_string(), JsonValue::from(total));
    if let Some(locator) = locator {
        if let Ok(encoded) = serde_json::to_value(locator) {
            map.insert("locator".to_string(), encoded);
        }
    }
    JsonValue::Object(map)
}

fn encoded_len(value: &JsonValue) -> usize {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(0)
}

/// Text the cell modal shows. Strings stay unquoted; objects are compact JSON.
pub fn cell_display_text(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => String::new(),
        JsonValue::String(text) => text.clone(),
        JsonValue::Bool(true) => "true".to_string(),
        JsonValue::Bool(false) => "false".to_string(),
        JsonValue::Number(number) => number.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// `None` means SQL `IS NULL`. Everything else is bound as text and compared
/// with a text cast, so an int8 past 2^53 keeps the decimal from P4.5.11.
fn key_text(value: &JsonValue) -> Option<String> {
    match value {
        JsonValue::Null => None,
        JsonValue::String(text) => Some(text.clone()),
        JsonValue::Bool(true) => Some("true".to_string()),
        JsonValue::Bool(false) => Some("false".to_string()),
        JsonValue::Number(number) => Some(number.to_string()),
        other => serde_json::to_string(other).ok(),
    }
}

fn nonempty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ColumnInfo;
    use serde_json::json;

    fn column(name: &str, pk: bool) -> ColumnInfo {
        ColumnInfo {
            name: name.to_string(),
            data_type: "text".to_string(),
            nullable: !pk,
            pk,
            enum_values: None,
            default_value: None,
            references: None,
        }
    }

    fn big_text() -> String {
        let mut text = "a".repeat(2_000);
        text.push_str("NEEDLE-TOKEN");
        text.push_str(&"b".repeat(40_000));
        text
    }

    #[test]
    fn short_cells_stay_intact() {
        let mut result = TablePreviewResult {
            engine: "sqlite".into(),
            columns: vec![column("id", true), column("body", false)],
            rows: vec![vec![json!(1), json!("hello")]],
            row_count: 1,
            total_rows: 0,
            total_known: false,
            truncated: false,
            duration_ms: 0,
        };
        clip_table_preview(&mut result, None, None, "docs");
        assert_eq!(result.rows[0][1], json!("hello"));
    }

    #[test]
    fn oversized_text_is_a_preview_without_the_payload() {
        let body = big_text();
        let mut result = TablePreviewResult {
            engine: "sqlite".into(),
            columns: vec![column("id", true), column("body", false)],
            rows: vec![vec![json!("9007199254740993"), JsonValue::String(body)]],
            row_count: 1,
            total_rows: 0,
            total_known: false,
            truncated: false,
            duration_ms: 0,
        };
        clip_table_preview(&mut result, None, None, "docs");
        let cell = &result.rows[0][1];
        let encoded = serde_json::to_string(cell).expect("marker json");
        assert!(
            encoded.len() < LARGE_CELL_BYTES,
            "{len}",
            len = encoded.len()
        );
        assert!(!encoded.contains("NEEDLE-TOKEN"), "{encoded}");
        assert_eq!(cell.get("$large").and_then(|v| v.as_bool()), Some(true));
        let preview = cell
            .get("preview")
            .and_then(|v| v.as_str())
            .expect("preview");
        assert_eq!(preview.chars().count(), LARGE_CELL_PREVIEW_CHARS + 1);
        assert!(preview.ends_with('\u{2026}'));
        let locator = cell.get("locator").expect("locator");
        assert_eq!(locator["table"], json!("docs"));
        assert_eq!(locator["column"], json!("body"));
        assert_eq!(locator["keys"][0]["column"], json!("id"));
        assert_eq!(locator["keys"][0]["value"], json!("9007199254740993"));
    }

    #[test]
    fn a_table_without_a_primary_key_has_no_locator() {
        let mut result = TablePreviewResult {
            engine: "sqlite".into(),
            columns: vec![column("body", false)],
            rows: vec![vec![JsonValue::String("x".repeat(40_000))]],
            row_count: 1,
            total_rows: 0,
            total_known: false,
            truncated: false,
            duration_ms: 0,
        };
        clip_table_preview(&mut result, None, None, "docs");
        assert!(result.rows[0][0].get("locator").is_none());
    }

    #[test]
    fn select_binds_the_key_instead_of_pasting_it_into_sql() {
        let locator = LargeCellLocator {
            database: None,
            schema: None,
            table: "docs".into(),
            column: "body".into(),
            keys: vec![LargeCellKey {
                column: "id".into(),
                value: json!("9007199254740993'); DROP TABLE docs;--"),
            }],
        };
        let select = large_cell_select(
            SqlDialect::Postgres,
            SqlPlaceholder::Dollar,
            SqlTextCast::Text,
            &locator,
        )
        .expect("sql");
        assert!(select.sql.contains("CAST(\"id\" AS text) = $1"));
        assert!(!select.sql.contains("DROP TABLE"));
        assert_eq!(
            select.params,
            vec!["9007199254740993'); DROP TABLE docs;--".to_string()]
        );
        let err = large_cell_select(
            SqlDialect::Postgres,
            SqlPlaceholder::Question,
            SqlTextCast::Text,
            &LargeCellLocator {
                keys: Vec::new(),
                ..locator
            },
        )
        .expect_err("no key");
        assert!(err.to_string().contains("primary key"));
    }

    #[test]
    fn paging_walks_characters_until_the_end() {
        let value = JsonValue::String("abcdef".into());
        let first = page_cell(&value, 0, 4);
        assert_eq!(first.text, "abcd");
        assert_eq!(first.next_offset, Some(4));
        assert_eq!(first.total_chars, 6);
        let rest = page_cell(&value, first.next_offset.expect("more"), 4);
        assert_eq!(rest.text, "ef");
        assert_eq!(rest.next_offset, None);
    }
}
