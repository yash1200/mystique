//! Large cell values cross IPC as a short preview. The full text is fetched
//! later by key, one chunk at a time.
//!
//! Agent and MCP keep their own character window. This clip runs only on the
//! grid IPC path, after the plugin has returned the real cell. File export
//! streams the plugin rows directly and never sees a preview.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};

use crate::cancel::QueryControl;
use crate::error::{ConnectorError, ConnectorResult};
use crate::origin::ResolvedColumn;
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
/// A single value bigger than this (UTF-8 bytes of its display text) is
/// refused rather than held in memory for the cell modal, a copy or an export.
pub const LARGE_VALUE_MAX_BYTES: usize = 64 * 1024 * 1024;

const ELLIPSIS: char = '\u{2026}';

/// One key value, already in the js-safe JSON form the grid uses.
/// `data_type` lets the engine bind it as that type (a MySQL integer key
/// is bound as an integer, a decimal one cast on the parameter side), so
/// the column is never cast and the key index is used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct LargeCellKey {
    pub column: String,
    pub value: JsonValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_type: Option<String>,
}

/// Handed back by the grid to load the rest of one cell. No SQL text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
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

/// `SELECT` plus the key parameters. Values are never interpolated.
#[derive(Debug, Clone, PartialEq)]
pub struct LargeCellSelect {
    pub sql: String,
    pub params: Vec<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct FetchLargeValueRequest {
    pub session_id: String,
    pub locator: LargeCellLocator,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub offset: u64,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub execution_id: Option<String>,
    /// A query tab with an open transaction reads the value through it, so
    /// a row the tab changed but has not committed still loads as shown.
    #[serde(default)]
    pub tab_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FetchLargeValueInput {
    pub locator: LargeCellLocator,
    pub offset: u64,
    pub limit: u64,
    pub control: Arc<QueryControl>,
}

impl FetchLargeValueRequest {
    pub fn chunk_limit(&self) -> u64 {
        self.limit
            .unwrap_or(LARGE_CELL_CHUNK_CHARS)
            .clamp(1, LARGE_CELL_CHUNK_CHARS)
    }

    pub fn into_input(self) -> FetchLargeValueInput {
        let limit = self.chunk_limit();
        FetchLargeValueInput {
            locator: self.locator,
            offset: self.offset,
            limit,
            control: QueryControl::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct FetchLargeValueResult {
    pub text: String,
    pub offset: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<u64>,
    pub total_chars: u64,
}

/// Build a single-cell read. An empty key list is refused: there is no
/// "match every column" fallback.
///
/// Each key is compared as `col = <param>` so the key index is used. The
/// parameter carries the type: Postgres infers it from the column (the value
/// is sent as text and parsed by that type's input function), MySQL casts
/// the parameter for integer and decimal keys, SQLite applies the column
/// affinity to the bound value.
pub fn large_cell_select(
    dialect: SqlDialect,
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
        let ident = quote_ident(dialect, &key.column);
        if key.value.is_null() {
            parts.push(format!("{ident} IS NULL"));
            continue;
        }
        let (slot, value) = match dialect {
            SqlDialect::Postgres => (format!("${}", params.len() + 1), key.value.clone()),
            SqlDialect::Mysql => mysql_key_param(key.data_type.as_deref(), &key.value),
            SqlDialect::Sqlite | SqlDialect::Generic => ("?".to_string(), key.value.clone()),
        };
        params.push(value);
        parts.push(format!("{ident} = {slot}"));
    }

    let col = quote_ident(dialect, &locator.column);
    let from = qualify_table(
        dialect,
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

/// MySQL compares a string parameter with an integer or decimal column as a
/// double, which loses digits past 2^53. An integer key is bound as an
/// integer (the grid carries big ones as decimal strings); a decimal key is
/// cast on the parameter side. The column itself is never cast, so the key
/// index is used.
fn mysql_key_param(data_type: Option<&str>, value: &JsonValue) -> (String, JsonValue) {
    let ty = data_type
        .map(|t| t.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let base = ty
        .split(|c: char| c == '(' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .to_string();
    match base.as_str() {
        "tinyint" | "smallint" | "mediumint" | "int" | "integer" | "bigint" => {
            let integer = value.as_str().and_then(|text| {
                let text = text.trim();
                text.parse::<i64>()
                    .map(JsonValue::from)
                    .or_else(|_| text.parse::<u64>().map(JsonValue::from))
                    .ok()
            });
            ("?".to_string(), integer.unwrap_or_else(|| value.clone()))
        }
        "decimal" | "numeric" | "dec" | "fixed" => {
            ("CAST(? AS DECIMAL(65,30))".to_string(), value.clone())
        }
        _ => ("?".to_string(), value.clone()),
    }
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
    let large = large_cells(&result.rows);
    if large.is_empty() {
        return;
    }
    let key: Vec<(usize, KeyRef)> = result
        .columns
        .iter()
        .enumerate()
        .filter(|(_, c)| c.pk)
        .map(|(index, c)| {
            (
                index,
                KeyRef {
                    column: c.name.clone(),
                    data_type: Some(c.data_type.clone()),
                },
            )
        })
        .collect();
    let target = Target {
        database: nonempty(database),
        schema: nonempty(schema),
        table: table.trim().to_string(),
    };
    for (row_index, col_index) in large {
        let row = &mut result.rows[row_index];
        let column = result
            .columns
            .get(col_index)
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let locator = if key.is_empty() || column.is_empty() || target.table.is_empty() {
            None
        } else {
            locator_for(&target, column, &key, row)
        };
        row[col_index] = marker(&row[col_index], locator);
    }
}

/// Clip a query result with no source information: previews without a locator.
pub fn clip_query_result(result: &mut QueryResult) {
    clip_query_result_with(result, &[]);
}

/// Clip a query result. `resolved[i]` is where column `i` came from. A cell
/// gets a locator when its table's key is fully present in the same row.
pub fn clip_query_result_with(result: &mut QueryResult, resolved: &[Option<ResolvedColumn>]) {
    let large = large_cells(&result.rows);
    for (row_index, col_index) in large {
        let row = &mut result.rows[row_index];
        let locator = resolved
            .get(col_index)
            .and_then(|r| r.as_ref())
            .and_then(|source| query_locator(source, resolved, row));
        row[col_index] = marker(&row[col_index], locator);
    }
}

pub fn clip_batch_result(result: &mut QueryBatchResult) {
    for stmt in &mut result.results {
        if let Some(query) = stmt.result.as_mut() {
            clip_query_result(query);
        }
    }
}

/// True when any cell would be replaced by a preview.
pub fn query_result_has_large(result: &QueryResult) -> bool {
    result.rows.iter().any(|row| row.iter().any(is_oversized))
}

struct Target {
    database: Option<String>,
    schema: Option<String>,
    table: String,
}

struct KeyRef {
    column: String,
    data_type: Option<String>,
}

fn query_locator(
    source: &ResolvedColumn,
    resolved: &[Option<ResolvedColumn>],
    row: &[JsonValue],
) -> Option<LargeCellLocator> {
    if source.table_key.is_empty() {
        return None;
    }
    let mut key = Vec::with_capacity(source.table_key.len());
    for part in &source.table_key {
        let index = resolved.iter().position(|other| {
            other
                .as_ref()
                .is_some_and(|other| other.same_table(source) && other.column == part.name)
        })?;
        key.push((
            index,
            KeyRef {
                column: part.name.clone(),
                data_type: part.data_type.clone(),
            },
        ));
    }
    let target = Target {
        database: source.database.clone(),
        schema: source.schema.clone(),
        table: source.table.clone(),
    };
    locator_for(&target, source.column.clone(), &key, row)
}

fn locator_for(
    target: &Target,
    column: String,
    key: &[(usize, KeyRef)],
    row: &[JsonValue],
) -> Option<LargeCellLocator> {
    let mut keys = Vec::with_capacity(key.len());
    for (index, part) in key {
        let value = row.get(*index).cloned().unwrap_or(JsonValue::Null);
        // A clipped or oversized key cannot be matched.
        if is_large_marker(&value) || encoded_len(&value) > LARGE_CELL_BYTES {
            return None;
        }
        keys.push(LargeCellKey {
            column: part.column.clone(),
            value,
            data_type: part.data_type.clone().filter(|t| !t.trim().is_empty()),
        });
    }
    Some(LargeCellLocator {
        database: target.database.clone(),
        schema: target.schema.clone(),
        table: target.table.clone(),
        column,
        keys,
    })
}

/// `(row, column)` of every cell that needs a preview.
fn large_cells(rows: &[Vec<JsonValue>]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            if is_oversized(cell) {
                out.push((r, c));
            }
        }
    }
    out
}

fn is_oversized(cell: &JsonValue) -> bool {
    !is_large_marker(cell) && encoded_len(cell) > LARGE_CELL_BYTES
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

/// Encoded JSON length. Short scalars skip the encode: a string can grow at
/// most six times (`\u0000`), so one that cannot reach the budget is not
/// encoded at all.
fn encoded_len(value: &JsonValue) -> usize {
    match value {
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => 0,
        JsonValue::String(text) if text.len().saturating_mul(6) + 2 <= LARGE_CELL_BYTES => {
            text.len() + 2
        }
        other => serde_json::to_vec(other)
            .map(|bytes| bytes.len())
            .unwrap_or(0),
    }
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

fn nonempty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::origin::KeyColumn;
    use crate::{ColumnInfo, QueryColumn, QueryMode};
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

    fn preview(columns: Vec<ColumnInfo>, rows: Vec<Vec<JsonValue>>) -> TablePreviewResult {
        TablePreviewResult {
            engine: "sqlite".into(),
            columns,
            row_count: rows.len() as u64,
            rows,
            total_rows: 0,
            total_known: false,
            truncated: false,
            duration_ms: 0,
        }
    }

    #[test]
    fn short_cells_stay_intact() {
        let mut result = preview(
            vec![column("id", true), column("body", false)],
            vec![vec![json!(1), json!("hello")]],
        );
        clip_table_preview(&mut result, None, None, "docs");
        assert_eq!(result.rows[0][1], json!("hello"));
    }

    #[test]
    fn oversized_text_is_a_preview_without_the_payload() {
        let body = big_text();
        let mut result = preview(
            vec![column("id", true), column("body", false)],
            vec![vec![json!("9007199254740993"), JsonValue::String(body)]],
        );
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
        assert_eq!(locator["keys"][0]["dataType"], json!("text"));
    }

    #[test]
    fn a_table_without_a_primary_key_has_no_locator() {
        let mut result = preview(
            vec![column("body", false)],
            vec![vec![JsonValue::String("x".repeat(40_000))]],
        );
        clip_table_preview(&mut result, None, None, "docs");
        assert!(result.rows[0][0].get("locator").is_none());
    }

    #[test]
    fn escaped_strings_are_measured_after_encoding() {
        // 6,000 control characters encode to 36,000 bytes.
        let text = "\u{1}".repeat(6_000);
        let mut result = preview(
            vec![column("id", true), column("body", false)],
            vec![vec![json!(1), JsonValue::String(text)]],
        );
        clip_table_preview(&mut result, None, None, "docs");
        assert!(is_large_marker(&result.rows[0][1]));
    }

    fn resolved(table: &str, column: &str, key: &[&str]) -> Option<ResolvedColumn> {
        Some(ResolvedColumn {
            database: None,
            schema: Some("public".into()),
            table: table.into(),
            column: column.into(),
            data_type: None,
            table_key: key
                .iter()
                .map(|name| KeyColumn {
                    name: (*name).into(),
                    data_type: Some("int8".into()),
                })
                .collect(),
        })
    }

    fn query(columns: &[&str], rows: Vec<Vec<JsonValue>>) -> QueryResult {
        QueryResult {
            engine: "postgres".into(),
            mode: QueryMode::Read,
            columns: columns
                .iter()
                .map(|name| QueryColumn {
                    name: (*name).into(),
                    data_type: None,
                })
                .collect(),
            row_count: rows.len() as u64,
            rows,
            truncated: false,
            duration_ms: 0,
            message: None,
        }
    }

    #[test]
    fn query_cells_get_a_locator_when_the_key_is_in_the_row() {
        let mut result = query(
            &["doc_id", "content"],
            vec![vec![json!(7), JsonValue::String(big_text())]],
        );
        let sources = vec![
            resolved("docs", "id", &["id"]),
            resolved("docs", "body", &["id"]),
        ];
        clip_query_result_with(&mut result, &sources);
        let locator = &result.rows[0][1]["locator"];
        assert_eq!(locator["table"], json!("docs"));
        assert_eq!(locator["schema"], json!("public"));
        // The source column name, not the alias.
        assert_eq!(locator["column"], json!("body"));
        assert_eq!(locator["keys"][0]["column"], json!("id"));
        assert_eq!(locator["keys"][0]["value"], json!(7));
    }

    #[test]
    fn query_cells_without_every_key_column_stay_previews() {
        let mut result = query(
            &["tenant", "content"],
            vec![vec![json!(1), JsonValue::String(big_text())]],
        );
        let sources = vec![
            resolved("docs", "tenant", &["tenant", "id"]),
            resolved("docs", "body", &["tenant", "id"]),
        ];
        clip_query_result_with(&mut result, &sources);
        assert!(is_large_marker(&result.rows[0][1]));
        assert!(result.rows[0][1].get("locator").is_none());

        // A key column from another table does not count.
        let mut joined = query(
            &["id", "content"],
            vec![vec![json!(1), JsonValue::String(big_text())]],
        );
        let sources = vec![
            resolved("users", "id", &["id"]),
            resolved("docs", "body", &["id"]),
        ];
        clip_query_result_with(&mut joined, &sources);
        assert!(joined.rows[0][1].get("locator").is_none());
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
                data_type: None,
            }],
        };
        let select = large_cell_select(SqlDialect::Postgres, &locator).expect("sql");
        assert_eq!(
            select.sql,
            "SELECT \"body\" FROM \"docs\" WHERE \"id\" = $1 LIMIT 1"
        );
        assert!(!select.sql.contains("DROP TABLE"));
        assert_eq!(
            select.params,
            vec![json!("9007199254740993'); DROP TABLE docs;--")]
        );
        let err = large_cell_select(
            SqlDialect::Postgres,
            &LargeCellLocator {
                keys: Vec::new(),
                ..locator
            },
        )
        .expect_err("no key");
        assert!(err.to_string().contains("primary key"));
    }

    #[test]
    fn the_key_column_is_never_wrapped_in_a_cast() {
        let key = |column: &str, value: JsonValue, data_type: &str| LargeCellKey {
            column: column.into(),
            value,
            data_type: Some(data_type.into()),
        };
        let locator = LargeCellLocator {
            database: Some("app".into()),
            schema: None,
            table: "docs".into(),
            column: "body".into(),
            keys: vec![
                key("id", json!("9007199254740993"), "bigint unsigned"),
                key("rev", json!(3), "int"),
                key("price", json!("1.10"), "decimal(10,2)"),
                key("slug", json!("00123"), "varchar(20)"),
                key("gone", JsonValue::Null, "int"),
            ],
        };
        let mysql = large_cell_select(SqlDialect::Mysql, &locator).expect("mysql");
        assert_eq!(
            mysql.sql,
            "SELECT `body` FROM `app`.`docs` WHERE `id` = ? AND `rev` = ? AND \
             `price` = CAST(? AS DECIMAL(65,30)) AND `slug` = ? AND `gone` IS NULL LIMIT 1"
        );
        // Integer keys travel as integers, even past 2^53; others as given.
        assert_eq!(
            mysql.params,
            vec![
                serde_json::Value::from(9_007_199_254_740_993_u64),
                json!(3),
                json!("1.10"),
                json!("00123")
            ]
        );
        let sqlite = large_cell_select(SqlDialect::Sqlite, &locator).expect("sqlite");
        assert!(!sqlite.sql.to_ascii_lowercase().contains("cast("));
        let pg = large_cell_select(SqlDialect::Postgres, &locator).expect("pg");
        assert!(pg.sql.contains("\"slug\" = $4 AND \"gone\" IS NULL"));
        assert!(!pg.sql.to_ascii_lowercase().contains("cast("));
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
