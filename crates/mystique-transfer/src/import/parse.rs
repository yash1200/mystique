//! Import preview: the first rows of a file as keys plus positional rows.
//!
//! CSV, JSON and JSON Lines go through the streaming reader, so a preview of a
//! huge file reads only as far as the rows it shows. A SQL dump is read whole.

use serde_json::Value as JsonValue;

use crate::encoding::{decode_file, ImportEncoding};
use crate::error::{TransferError, TransferResult};

use super::sql_dump::parse_sql;
use super::stream::{stream_import_file, ImportRecord};
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

pub(super) fn pad_row(mut row: Vec<JsonValue>, width: usize) -> Vec<JsonValue> {
    row.resize(width, JsonValue::Null);
    row
}

/// The first `limit` rows (all rows when `None`). Keys are every key seen in
/// those rows, in first-seen order.
pub fn parse_import_file(
    path: &str,
    format: ImportFormat,
    encoding: ImportEncoding,
    limit: Option<usize>,
) -> TransferResult<ParsedTable> {
    if format == ImportFormat::Sql {
        let text = decode_file(path, encoding)?;
        return parse_sql(&text, limit);
    }
    let want = limit.unwrap_or(usize::MAX);
    let mut records: Vec<ImportRecord> = Vec::new();
    stream_import_file(path, format, encoding, want.min(1_000), &mut |batch| {
        let room = want.saturating_sub(records.len());
        records.extend(batch.into_iter().take(room));
        records.len() < want
    })?;
    let mut keys: Vec<String> = Vec::new();
    for record in &records {
        for key in record.keys() {
            if !keys.iter().any(|k| k == key) {
                keys.push(key.clone());
            }
        }
    }
    if keys.is_empty() {
        if format == ImportFormat::Csv {
            keys = csv_header(path, encoding)?;
        } else {
            return Err(TransferError::InvalidConfig(
                "The file has no rows with keys.".into(),
            ));
        }
    }
    let rows = records
        .iter()
        .map(|record| {
            keys.iter()
                .map(|k| record.get(k).cloned().unwrap_or(JsonValue::Null))
                .collect()
        })
        .collect();
    Ok(ParsedTable { keys, rows })
}

/// A CSV with a header and no rows still has keys.
fn csv_header(path: &str, encoding: ImportEncoding) -> TransferResult<Vec<String>> {
    let text = decode_file(path, encoding)?;
    let first = text.lines().next().unwrap_or("");
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .trim(csv::Trim::All)
        .from_reader(first.as_bytes());
    let mut record = csv::StringRecord::new();
    let found = rdr
        .read_record(&mut record)
        .map_err(|e| TransferError::InvalidConfig(format!("csv header: {e}")))?;
    if !found || record.is_empty() {
        return Err(TransferError::InvalidConfig("CSV has no columns.".into()));
    }
    Ok(record
        .iter()
        .enumerate()
        .map(|(i, h)| {
            if h.trim().is_empty() {
                format!("col_{}", i + 1)
            } else {
                h.trim().to_string()
            }
        })
        .collect())
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preview_reads_only_the_rows_it_shows_and_keeps_key_order() {
        let mut text = String::from("b,a\n");
        for i in 0..5_000 {
            text.push_str(&format!("{i},x\n"));
        }
        let path =
            std::env::temp_dir().join(format!("mystique-preview-{}.csv", std::process::id()));
        std::fs::write(&path, text).expect("write");
        let parsed = parse_import_file(
            &path.to_string_lossy(),
            ImportFormat::Csv,
            ImportEncoding::Utf8,
            Some(100),
        )
        .expect("parse");
        assert_eq!(parsed.keys, vec!["b".to_string(), "a".to_string()]);
        assert_eq!(parsed.rows.len(), 100);
        assert_eq!(parsed.rows[99], vec![json!(99), json!("x")]);

        std::fs::write(&path, "id,name\n").expect("write");
        let empty = parse_import_file(
            &path.to_string_lossy(),
            ImportFormat::Csv,
            ImportEncoding::Utf8,
            Some(100),
        )
        .expect("header only");
        assert_eq!(empty.keys, vec!["id".to_string(), "name".to_string()]);
        assert!(empty.rows.is_empty());
        let _ = std::fs::remove_file(&path);
    }
}
