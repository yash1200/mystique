//! Plugin-owned table/collection file import.
//!
//! The FE picks a file + encoding + column map. Plugins write 100-row chunks.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};

use crate::catalog::ColumnInfo;
use crate::connector::ConnectorSession;
use crate::edits::{ApplyTableEditsInput, TableEditColumn, TableRowInsert};
use crate::error::{ConnectorError, ConnectorResult};

pub const IMPORT_CHUNK_SIZE: usize = 100;
pub const IMPORT_PREVIEW_ROWS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportFormat {
    Csv,
    Json,
    Jsonl,
    Sql,
}

impl ImportFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Jsonl => "jsonl",
            Self::Sql => "sql",
        }
    }

    pub fn from_path(path: &str) -> Option<Self> {
        let ext = Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "csv" => Some(Self::Csv),
            "jsonl" | "ndjson" => Some(Self::Jsonl),
            "json" => Some(Self::Json),
            "sql" => Some(Self::Sql),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Windows1252,
    Iso88591,
}

impl ImportEncoding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Utf8 => "utf-8",
            Self::Utf16Le => "utf-16le",
            Self::Utf16Be => "utf-16be",
            Self::Windows1252 => "windows-1252",
            Self::Iso88591 => "iso-8859-1",
        }
    }

    fn decoder(self) -> &'static encoding_rs::Encoding {
        match self {
            Self::Utf8 => encoding_rs::UTF_8,
            Self::Utf16Le => encoding_rs::UTF_16LE,
            Self::Utf16Be => encoding_rs::UTF_16BE,
            Self::Windows1252 | Self::Iso88591 => encoding_rs::WINDOWS_1252,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportOnDuplicate {
    StopOnError,
    UpdateOnDuplicate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportColumnMap {
    pub file_key: String,
    #[serde(default)]
    pub table_column: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewImportRequest {
    pub session_id: String,
    pub name: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    pub path: String,
    pub format: ImportFormat,
    #[serde(default)]
    pub encoding: ImportEncoding,
}

#[derive(Debug, Clone)]
pub struct PreviewImportInput {
    pub name: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub kind: Option<String>,
    pub path: String,
    pub format: ImportFormat,
    pub encoding: ImportEncoding,
}

impl PreviewImportRequest {
    pub fn into_input(self) -> PreviewImportInput {
        PreviewImportInput {
            name: self.name,
            schema: self.schema,
            database: self.database,
            kind: self.kind,
            path: self.path,
            format: self.format,
            encoding: self.encoding,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportTableColumn {
    pub name: String,
    pub data_type: String,
    pub pk: bool,
    pub nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewImportResult {
    pub file_keys: Vec<String>,
    pub columns: Vec<ImportTableColumn>,
    pub suggested_map: Vec<ImportColumnMap>,
    pub rows: Vec<Vec<JsonValue>>,
    pub format: ImportFormat,
    pub encoding: ImportEncoding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportObjectRequest {
    pub session_id: String,
    pub name: String,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub database: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    pub path: String,
    pub format: ImportFormat,
    #[serde(default)]
    pub encoding: ImportEncoding,
    #[serde(default)]
    pub mapping: Vec<ImportColumnMap>,
    #[serde(default)]
    pub on_duplicate: ImportOnDuplicate,
    #[serde(default)]
    pub profile_read_only: bool,
}

#[derive(Debug, Clone)]
pub struct ImportObjectInput {
    pub name: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub kind: Option<String>,
    pub path: String,
    pub format: ImportFormat,
    pub encoding: ImportEncoding,
    pub mapping: Vec<ImportColumnMap>,
    pub on_duplicate: ImportOnDuplicate,
}

impl ImportObjectRequest {
    pub fn into_input(self) -> ImportObjectInput {
        ImportObjectInput {
            name: self.name,
            schema: self.schema,
            database: self.database,
            kind: self.kind,
            path: self.path,
            format: self.format,
            encoding: self.encoding,
            mapping: self.mapping,
            on_duplicate: self.on_duplicate,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportObjectResult {
    pub inserted: u64,
    pub updated: u64,
    pub format: ImportFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ImportChunkInput {
    pub name: String,
    pub schema: Option<String>,
    pub database: Option<String>,
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<BTreeMap<String, JsonValue>>,
    pub on_duplicate: ImportOnDuplicate,
}

#[derive(Debug, Clone, Default)]
pub struct ImportChunkResult {
    pub inserted: u64,
    pub updated: u64,
}

impl Default for ImportEncoding {
    fn default() -> Self {
        Self::Utf8
    }
}

impl Default for ImportOnDuplicate {
    fn default() -> Self {
        Self::StopOnError
    }
}

fn decode_file(path: &str, encoding: ImportEncoding) -> ConnectorResult<String> {
    let bytes = std::fs::read(path)
        .map_err(|e| ConnectorError::InvalidConfig(format!("could not read {path}: {e}")))?;
    let (cow, _, _) = encoding.decoder().decode(&bytes);
    Ok(cow.into_owned())
}

fn parse_cell(raw: &str) -> JsonValue {
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

struct ParsedTable {
    keys: Vec<String>,
    rows: Vec<Vec<JsonValue>>,
}

fn parse_csv(text: &str, limit: Option<usize>) -> ConnectorResult<ParsedTable> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(Cursor::new(text.as_bytes()));
    let headers = rdr
        .headers()
        .map_err(|e| ConnectorError::InvalidConfig(format!("csv header: {e}")))?
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
        return Err(ConnectorError::InvalidConfig("CSV has no columns.".into()));
    }
    let width = headers.len();
    let mut rows = Vec::new();
    for rec in rdr.records() {
        let rec = rec.map_err(|e| ConnectorError::InvalidConfig(format!("csv row: {e}")))?;
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

fn object_keys(v: &Map<String, JsonValue>) -> Vec<String> {
    v.keys().cloned().collect()
}

fn row_from_object(keys: &[String], obj: &Map<String, JsonValue>) -> Vec<JsonValue> {
    keys.iter()
        .map(|k| obj.get(k).cloned().unwrap_or(JsonValue::Null))
        .collect()
}

fn parse_json(text: &str, limit: Option<usize>) -> ConnectorResult<ParsedTable> {
    let value: JsonValue = serde_json::from_str(text.trim())
        .map_err(|e| ConnectorError::InvalidConfig(format!("json: {e}")))?;
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
                return Err(ConnectorError::InvalidConfig(
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
        JsonValue::Array(items)
            if items.first().is_some_and(|v| v.is_array()) =>
        {
            let mut iter = items.into_iter();
            let first = iter.next().and_then(|v| match v {
                JsonValue::Array(a) => Some(a),
                _ => None,
            });
            let Some(first) = first else {
                return Err(ConnectorError::InvalidConfig("JSON array is empty.".into()));
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
        _ => Err(ConnectorError::InvalidConfig(
            "JSON must be an object, an array of objects, or an array of arrays.".into(),
        )),
    }
}

fn pad_row(mut row: Vec<JsonValue>, width: usize) -> Vec<JsonValue> {
    row.resize(width, JsonValue::Null);
    row
}

fn parse_jsonl(text: &str, limit: Option<usize>) -> ConnectorResult<ParsedTable> {
    let mut keys = Vec::new();
    let mut objects = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: JsonValue = serde_json::from_str(line).map_err(|e| {
            ConnectorError::InvalidConfig(format!("jsonl line {}: {e}", i + 1))
        })?;
        let obj = v.as_object().ok_or_else(|| {
            ConnectorError::InvalidConfig(format!("jsonl line {} is not an object", i + 1))
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
        return Err(ConnectorError::InvalidConfig("JSON Lines file is empty.".into()));
    }
    let rows = objects
        .iter()
        .map(|o| row_from_object(&keys, o))
        .collect();
    Ok(ParsedTable { keys, rows })
}

fn strip_ident(raw: &str) -> String {
    raw.trim()
        .trim_matches(|c| c == '"' || c == '`' || c == '[' || c == ']')
        .to_string()
}

fn parse_ident_list(inner: &str) -> Vec<String> {
    inner
        .split(',')
        .map(strip_ident)
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_sql_string(s: &str, i: &mut usize) -> Option<String> {
    let bytes = s.as_bytes();
    if *i >= bytes.len() || bytes[*i] != b'\'' {
        return None;
    }
    *i += 1;
    let mut out = String::new();
    while *i < bytes.len() {
        if bytes[*i] == b'\'' {
            if *i + 1 < bytes.len() && bytes[*i + 1] == b'\'' {
                out.push('\'');
                *i += 2;
                continue;
            }
            *i += 1;
            return Some(out);
        }
        out.push(bytes[*i] as char);
        *i += 1;
    }
    None
}

fn parse_sql_value(s: &str, i: &mut usize) -> Option<JsonValue> {
    let bytes = s.as_bytes();
    while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
        *i += 1;
    }
    if *i >= bytes.len() {
        return None;
    }
    match bytes[*i] {
        b'\'' => parse_sql_string(s, i).map(JsonValue::String),
        b'N' | b'n' => {
            let rest = &s[*i..];
            if rest.len() >= 4 && rest[..4].eq_ignore_ascii_case("null") {
                *i += 4;
                Some(JsonValue::Null)
            } else {
                None
            }
        }
        b'T' | b't' => {
            let rest = &s[*i..];
            if rest.len() >= 4 && rest[..4].eq_ignore_ascii_case("true") {
                *i += 4;
                Some(JsonValue::Bool(true))
            } else {
                None
            }
        }
        b'F' | b'f' => {
            let rest = &s[*i..];
            if rest.len() >= 5 && rest[..5].eq_ignore_ascii_case("false") {
                *i += 5;
                Some(JsonValue::Bool(false))
            } else {
                None
            }
        }
        b'"' | b'`' => {
            let q = bytes[*i];
            *i += 1;
            let start = *i;
            while *i < bytes.len() && bytes[*i] != q {
                *i += 1;
            }
            let s = s[start..*i].to_string();
            if *i < bytes.len() {
                *i += 1;
            }
            Some(JsonValue::String(s))
        }
        _ => {
            let start = *i;
            while *i < bytes.len()
                && !matches!(bytes[*i], b',' | b')' | b';')
                && !bytes[*i].is_ascii_whitespace()
            {
                *i += 1;
            }
            Some(parse_cell(&s[start..*i]))
        }
    }
}

fn parse_sql_tuple(s: &str, i: &mut usize) -> Option<Vec<JsonValue>> {
    let bytes = s.as_bytes();
    while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
        *i += 1;
    }
    if *i >= bytes.len() || bytes[*i] != b'(' {
        return None;
    }
    *i += 1;
    let mut vals = Vec::new();
    loop {
        while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
            *i += 1;
        }
        if *i < bytes.len() && bytes[*i] == b')' {
            *i += 1;
            return Some(vals);
        }
        vals.push(parse_sql_value(s, i)?);
        while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
            *i += 1;
        }
        if *i < bytes.len() && bytes[*i] == b',' {
            *i += 1;
            continue;
        }
        if *i < bytes.len() && bytes[*i] == b')' {
            *i += 1;
            return Some(vals);
        }
        return None;
    }
}

fn parse_sql(text: &str, limit: Option<usize>) -> ConnectorResult<ParsedTable> {
    let mut keys: Vec<String> = Vec::new();
    let mut rows = Vec::new();
    let lower = text.to_ascii_lowercase();
    let mut search = 0usize;
    while let Some(rel) = lower[search..].find("insert") {
        let start = search + rel;
        let after = &text[start..];
        let after_l = &lower[start..];
        let Some(values_rel) = after_l.find("values") else {
            break;
        };
        let head = &after[..values_rel];
        let mut col_keys = Vec::new();
        if let (Some(open), Some(close)) = (head.rfind('('), head.rfind(')')) {
            if open < close {
                col_keys = parse_ident_list(&head[open + 1..close]);
            }
        }
        let mut i = start + values_rel + "values".len();
        while let Some(tuple) = parse_sql_tuple(text, &mut i) {
            if keys.is_empty() {
                keys = if col_keys.is_empty() {
                    (0..tuple.len())
                        .map(|n| format!("col_{}", n + 1))
                        .collect()
                } else {
                    col_keys.clone()
                };
            }
            rows.push(pad_row(tuple, keys.len()));
            if limit.is_some_and(|n| rows.len() >= n) {
                return Ok(ParsedTable { keys, rows });
            }
            while i < text.len() && text.as_bytes()[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < text.len() && text.as_bytes()[i] == b',' {
                i += 1;
                continue;
            }
            break;
        }
        search = i.max(start + 6);
        if search >= text.len() {
            break;
        }
    }
    if keys.is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "No INSERT rows found in the SQL file.".into(),
        ));
    }
    Ok(ParsedTable { keys, rows })
}

fn parse_import_file(
    path: &str,
    format: ImportFormat,
    encoding: ImportEncoding,
    limit: Option<usize>,
) -> ConnectorResult<ParsedTable> {
    let text = decode_file(path, encoding)?;
    match format {
        ImportFormat::Csv => parse_csv(&text, limit),
        ImportFormat::Json => parse_json(&text, limit),
        ImportFormat::Jsonl => parse_jsonl(&text, limit),
        ImportFormat::Sql => parse_sql(&text, limit),
    }
}

fn suggest_map(file_keys: &[String], columns: &[ColumnInfo]) -> Vec<ImportColumnMap> {
    file_keys
        .iter()
        .map(|key| {
            let table_column = columns.iter().find(|c| c.name.eq_ignore_ascii_case(key));
            ImportColumnMap {
                file_key: key.clone(),
                table_column: table_column.map(|c| c.name.clone()),
            }
        })
        .collect()
}

pub async fn preview_import_with_describe(
    session: &dyn ConnectorSession,
    input: &PreviewImportInput,
) -> ConnectorResult<PreviewImportResult> {
    if input.path.trim().is_empty() {
        return Err(ConnectorError::InvalidConfig("Import path is required.".into()));
    }
    let columns = session
        .describe_table(
            input.database.as_deref(),
            input.schema.as_deref(),
            &input.name,
        )
        .await?;
    let parsed = parse_import_file(
        &input.path,
        input.format,
        input.encoding,
        Some(IMPORT_PREVIEW_ROWS),
    )?;
    let suggested_map = suggest_map(&parsed.keys, &columns);
    Ok(PreviewImportResult {
        file_keys: parsed.keys,
        columns: columns
            .into_iter()
            .map(|c| ImportTableColumn {
                name: c.name,
                data_type: c.data_type,
                pk: c.pk,
                nullable: c.nullable,
            })
            .collect(),
        suggested_map,
        rows: parsed.rows,
        format: input.format,
        encoding: input.encoding,
    })
}

fn remap_row(
    keys: &[String],
    row: &[JsonValue],
    mapping: &[ImportColumnMap],
) -> BTreeMap<String, JsonValue> {
    let mut out = BTreeMap::new();
    for map in mapping {
        let Some(col) = map
            .table_column
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let idx = keys.iter().position(|k| k == &map.file_key);
        let val = idx
            .and_then(|i| row.get(i))
            .cloned()
            .unwrap_or(JsonValue::Null);
        out.insert(col.to_string(), val);
    }
    out
}

pub async fn import_object_from_file(
    session: &dyn ConnectorSession,
    input: &ImportObjectInput,
) -> ConnectorResult<ImportObjectResult> {
    if input.path.trim().is_empty() {
        return Err(ConnectorError::InvalidConfig("Import path is required.".into()));
    }
    if input.name.trim().is_empty() {
        return Err(ConnectorError::InvalidConfig("Object name is required.".into()));
    }
    let columns = session
        .describe_table(
            input.database.as_deref(),
            input.schema.as_deref(),
            &input.name,
        )
        .await?;
    if columns.is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "Target object has no columns.".into(),
        ));
    }
    let parsed = parse_import_file(&input.path, input.format, input.encoding, None)?;
    let mapping = if input.mapping.is_empty() {
        suggest_map(&parsed.keys, &columns)
    } else {
        input.mapping.clone()
    };
    if mapping.iter().all(|m| {
        m.table_column
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    }) {
        return Err(ConnectorError::InvalidConfig(
            "Map at least one file field to a column.".into(),
        ));
    }

    let mut inserted = 0u64;
    let mut updated = 0u64;
    let mut chunk: Vec<BTreeMap<String, JsonValue>> = Vec::with_capacity(IMPORT_CHUNK_SIZE);
    for row in &parsed.rows {
        let mapped = remap_row(&parsed.keys, row, &mapping);
        if mapped.is_empty() {
            continue;
        }
        chunk.push(mapped);
        if chunk.len() >= IMPORT_CHUNK_SIZE {
            let result = session
                .import_chunk(&ImportChunkInput {
                    name: input.name.clone(),
                    schema: input.schema.clone(),
                    database: input.database.clone(),
                    columns: columns.clone(),
                    rows: std::mem::take(&mut chunk),
                    on_duplicate: input.on_duplicate,
                })
                .await?;
            inserted += result.inserted;
            updated += result.updated;
        }
    }
    if !chunk.is_empty() {
        let result = session
            .import_chunk(&ImportChunkInput {
                name: input.name.clone(),
                schema: input.schema.clone(),
                database: input.database.clone(),
                columns: columns.clone(),
                rows: chunk,
                on_duplicate: input.on_duplicate,
            })
            .await?;
        inserted += result.inserted;
        updated += result.updated;
    }

    Ok(ImportObjectResult {
        inserted,
        updated,
        format: input.format,
        message: Some(format!(
            "Imported {} row(s) ({} inserted, {} updated).",
            inserted + updated,
            inserted,
            updated
        )),
    })
}

/// Shared SQL write for one import chunk (insert or upsert).
pub fn import_chunk_sql(
    style: crate::sql_table_edits::SqlIdentStyle,
    chunk: &ImportChunkInput,
) -> ConnectorResult<(Vec<String>, ImportChunkResult)> {
    let edits = ApplyTableEditsInput {
        table: chunk.name.clone(),
        schema: chunk.schema.clone(),
        database: chunk.database.clone(),
        columns: chunk
            .columns
            .iter()
            .map(|c| TableEditColumn {
                name: c.name.clone(),
                data_type: c.data_type.clone(),
                nullable: c.nullable,
                pk: c.pk,
            })
            .collect(),
        updates: Vec::new(),
        inserts: Vec::new(),
        deletes: Vec::new(),
    };
    let mut statements = Vec::new();
    let mut result = ImportChunkResult::default();
    for row in &chunk.rows {
        let insert = TableRowInsert {
            values: row.clone(),
        };
        match chunk.on_duplicate {
            ImportOnDuplicate::StopOnError => {
                statements.push(crate::sql_table_edits::compile_insert(
                    style, &edits, &insert,
                )?);
                result.inserted += 1;
            }
            ImportOnDuplicate::UpdateOnDuplicate => {
                statements.push(crate::sql_table_edits::compile_upsert(
                    style, &edits, &insert,
                )?);
                result.updated += 1;
            }
        }
    }
    Ok((statements, result))
}
