//! Decode a file and parse CSV / JSON / JSONL tables.

use std::io::Cursor;

use serde_json::{Map, Value as JsonValue};

use crate::encoding::{decode_file, ImportEncoding};
use crate::error::{TransferError, TransferResult};

use super::sql_dump::parse_sql;
use super::{ImportColumnMap, ImportFormat};

/// File keys (CSV header, JSON object keys) and rows in that key order.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTable {
    pub keys: Vec<String>,
    pub rows: Vec<Vec<JsonValue>>,
}

pub(super) fn parse_cell(raw: &str) -> JsonValue {
    let t = raw.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("null") {
        return JsonValue::Null;
    }
    if t.eq_ignore_ascii_case("true") {
        return JsonValue::Bool(true);
    }
    if t.eq_ignore_ascii_case("false") {
        return JsonValue::Bool(false);
    }
    if let Ok(n) = t.parse::<i64>() {
        return JsonValue::Number(n.into());
    }
    if let Ok(f) = t.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return JsonValue::Number(n);
        }
    }
    JsonValue::String(raw.to_string())
}

pub(super) fn parse_csv(text: &str, limit: Option<usize>) -> TransferResult<ParsedTable> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(Cursor::new(text.as_bytes()));
    let headers = rdr
        .headers()
        .map_err(|e| TransferError::InvalidConfig(format!("csv header: {e}")))?
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let t = h.trim();
            if t.is_empty() {
                format!("col_{}", i + 1)
            } else {
                t.to_string()
            }
        })
        .collect::<Vec<_>>();
    if headers.is_empty() {
        return Err(TransferError::InvalidConfig("CSV has no columns.".into()));
    }
    let width = headers.len();
    let mut rows = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| TransferError::InvalidConfig(format!("csv row: {e}")))?;
        let mut row = Vec::with_capacity(width);
        for i in 0..width {
            row.push(parse_cell(rec.get(i).unwrap_or("")));
        }
        rows.push(row);
        if limit.is_some_and(|n| rows.len() >= n) {
            break;
        }
    }
    Ok(ParsedTable {
        keys: headers,
        rows,
    })
}

pub(super) fn object_keys(v: &Map<String, JsonValue>) -> Vec<String> {
    v.keys().cloned().collect()
}

pub(super) fn row_from_object(keys: &[String], obj: &Map<String, JsonValue>) -> Vec<JsonValue> {
    keys.iter()
        .map(|k| obj.get(k).cloned().unwrap_or(JsonValue::Null))
        .collect()
}

pub(super) fn parse_json(text: &str, limit: Option<usize>) -> TransferResult<ParsedTable> {
    let value: JsonValue = serde_json::from_str(text.trim())
        .map_err(|e| TransferError::InvalidConfig(format!("json: {e}")))?;
    match value {
        JsonValue::Array(items) if items.iter().all(|v| v.is_object()) => {
            let mut keys = Vec::new();
            for item in &items {
                if let JsonValue::Object(o) = item {
                    for k in o.keys() {
                        if !keys.iter().any(|x| x == k) {
                            keys.push(k.clone());
                        }
                    }
                }
            }
            if keys.is_empty() {
                return Err(TransferError::InvalidConfig(
                    "JSON array has no object keys.".into(),
                ));
            }
            let take = limit.unwrap_or(items.len());
            let rows = items
                .iter()
                .take(take)
                .filter_map(|v| v.as_object())
                .map(|o| row_from_object(&keys, o))
                .collect();
            Ok(ParsedTable { keys, rows })
        }
        JsonValue::Object(o) => {
            let keys = object_keys(&o);
            Ok(ParsedTable {
                keys: keys.clone(),
                rows: vec![row_from_object(&keys, &o)],
            })
        }
        JsonValue::Array(items) if items.first().is_some_and(|v| v.is_array()) => {
            let mut iter = items.into_iter();
            let first = iter.next().and_then(|v| match v {
                JsonValue::Array(a) => Some(a),
                _ => None,
            });
            let Some(first) = first else {
                return Err(TransferError::InvalidConfig("JSON array is empty.".into()));
            };
            let all_strings = first.iter().all(|c| c.is_string());
            let (keys, first_data) = if all_strings {
                (
                    first
                        .iter()
                        .enumerate()
                        .map(|(i, c)| {
                            c.as_str()
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .unwrap_or_else(|| format!("col_{}", i + 1))
                        })
                        .collect::<Vec<_>>(),
                    None,
                )
            } else {
                (
                    (0..first.len())
                        .map(|i| format!("col_{}", i + 1))
                        .collect::<Vec<_>>(),
                    Some(first),
                )
            };
            let mut rows = Vec::new();
            if let Some(row) = first_data {
                rows.push(pad_row(row, keys.len()));
            }
            let take = limit.unwrap_or(usize::MAX);
            for item in iter {
                if rows.len() >= take {
                    break;
                }
                if let JsonValue::Array(a) = item {
                    rows.push(pad_row(a, keys.len()));
                }
            }
            Ok(ParsedTable { keys, rows })
        }
        _ => Err(TransferError::InvalidConfig(
            "JSON must be an object, an array of objects, or an array of arrays.".into(),
        )),
    }
}

pub(super) fn pad_row(mut row: Vec<JsonValue>, width: usize) -> Vec<JsonValue> {
    row.resize(width, JsonValue::Null);
    row
}

pub(super) fn parse_jsonl(text: &str, limit: Option<usize>) -> TransferResult<ParsedTable> {
    let mut keys = Vec::new();
    let mut objects = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: JsonValue = serde_json::from_str(line)
            .map_err(|e| TransferError::InvalidConfig(format!("jsonl line {}: {e}", i + 1)))?;
        let obj = v.as_object().ok_or_else(|| {
            TransferError::InvalidConfig(format!("jsonl line {} is not an object", i + 1))
        })?;
        for k in obj.keys() {
            if !keys.iter().any(|x| x == k) {
                keys.push(k.clone());
            }
        }
        objects.push(obj.clone());
        if limit.is_some_and(|n| objects.len() >= n) {
            break;
        }
    }
    if keys.is_empty() {
        return Err(TransferError::InvalidConfig(
            "JSON Lines file is empty.".into(),
        ));
    }
    let rows = objects.iter().map(|o| row_from_object(&keys, o)).collect();
    Ok(ParsedTable { keys, rows })
}

pub fn parse_import_file(
    path: &str,
    format: ImportFormat,
    encoding: ImportEncoding,
    limit: Option<usize>,
) -> TransferResult<ParsedTable> {
    let text = decode_file(path, encoding)?;
    match format {
        ImportFormat::Csv => parse_csv(&text, limit),
        ImportFormat::Json => parse_json(&text, limit),
        ImportFormat::Jsonl => parse_jsonl(&text, limit),
        ImportFormat::Sql => parse_sql(&text, limit),
    }
}

/// Map each file key to the table column with the same name, ignoring case.
pub fn suggest_map(file_keys: &[String], column_names: &[&str]) -> Vec<ImportColumnMap> {
    file_keys
        .iter()
        .map(|key| {
            let table_column = column_names
                .iter()
                .find(|name| name.eq_ignore_ascii_case(key));
            ImportColumnMap {
                file_key: key.clone(),
                table_column: table_column.map(|name| name.to_string()),
            }
        })
        .collect()
}
