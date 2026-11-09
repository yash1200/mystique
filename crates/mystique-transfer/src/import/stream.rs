//! Streaming import: decode and parse a file record by record.
//!
//! CSV, JSON Lines and JSON (an array of objects or of arrays, or one
//! object) are read incrementally, so memory is one batch of records however
//! big the file is. A SQL dump is still read whole (`INSERT` statements can
//! span lines and the dump parser works on text), then handed out in batches.

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::sync::Arc;

use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value as JsonValue};

use crate::encoding::ImportEncoding;
use crate::error::{TransferError, TransferResult};

use super::parse::parse_cell;
use super::ImportFormat;

/// One file record: file keys and their values, in file order (a CSV header
/// is shared by every record of the file).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportRecord {
    keys: Arc<[String]>,
    values: Vec<JsonValue>,
}

impl ImportRecord {
    pub fn new(keys: Arc<[String]>, mut values: Vec<JsonValue>) -> Self {
        values.resize(keys.len(), JsonValue::Null);
        Self { keys, values }
    }

    fn from_object(object: Map<String, JsonValue>) -> Self {
        let (keys, values): (Vec<String>, Vec<JsonValue>) = object.into_iter().unzip();
        Self {
            keys: keys.into(),
            values,
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.keys.iter()
    }

    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        let index = self.keys.iter().position(|k| k == key)?;
        self.values.get(index)
    }
}

/// Called with each batch. Return `false` to stop reading (the importer was
/// cancelled, or a preview has enough rows).
pub type OnBatch<'a> = dyn FnMut(Vec<ImportRecord>) -> bool + 'a;

/// Read `path` and hand records to `on_batch` in batches of `batch` records.
/// Returns the number of records handed out.
pub fn stream_import_file(
    path: &str,
    format: ImportFormat,
    encoding: ImportEncoding,
    batch: usize,
    on_batch: &mut OnBatch<'_>,
) -> TransferResult<u64> {
    let file = File::open(path)
        .map_err(|e| TransferError::InvalidConfig(format!("could not read {path}: {e}")))?;
    let reader = BufReader::with_capacity(256 * 1024, DecodeReader::new(file, encoding));
    let mut out = Batcher::new(batch.max(1), on_batch);
    match format {
        ImportFormat::Csv => stream_csv(reader, &mut out)?,
        ImportFormat::Jsonl => stream_jsonl(reader, &mut out)?,
        ImportFormat::Json => stream_json(reader, &mut out)?,
        ImportFormat::Sql => {
            let mut text = String::new();
            let mut reader = reader;
            reader
                .read_to_string(&mut text)
                .map_err(|e| TransferError::InvalidConfig(format!("could not read {path}: {e}")))?;
            let table = super::sql_dump::parse_sql(&text, None)?;
            drop(text);
            let keys: Arc<[String]> = table.keys.into();
            for row in table.rows {
                if !out.push(ImportRecord::new(Arc::clone(&keys), row)) {
                    break;
                }
            }
        }
    }
    out.finish();
    Ok(out.count)
}

struct Batcher<'a, 'b> {
    size: usize,
    batch: Vec<ImportRecord>,
    on_batch: &'a mut OnBatch<'b>,
    stopped: bool,
    count: u64,
}

impl<'a, 'b> Batcher<'a, 'b> {
    fn new(size: usize, on_batch: &'a mut OnBatch<'b>) -> Self {
        Self {
            size,
            batch: Vec::with_capacity(size),
            on_batch,
            stopped: false,
            count: 0,
        }
    }

    /// False once the receiver asked to stop.
    fn push(&mut self, record: ImportRecord) -> bool {
        if self.stopped {
            return false;
        }
        self.batch.push(record);
        self.count += 1;
        if self.batch.len() >= self.size {
            let full = std::mem::replace(&mut self.batch, Vec::with_capacity(self.size));
            if !(self.on_batch)(full) {
                self.stopped = true;
            }
        }
        !self.stopped
    }

    fn finish(&mut self) {
        if !self.stopped && !self.batch.is_empty() {
            let rest = std::mem::take(&mut self.batch);
            (self.on_batch)(rest);
        }
    }
}

fn stream_csv(reader: impl Read, out: &mut Batcher<'_, '_>) -> TransferResult<()> {
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(reader);
    let headers: Arc<[String]> = rdr
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
        .collect();
    if headers.is_empty() {
        return Err(TransferError::InvalidConfig("CSV has no columns.".into()));
    }
    let mut record = csv::StringRecord::new();
    loop {
        let more = rdr
            .read_record(&mut record)
            .map_err(|e| TransferError::InvalidConfig(format!("csv row: {e}")))?;
        if !more {
            return Ok(());
        }
        let values = (0..headers.len())
            .map(|i| parse_cell(record.get(i).unwrap_or("")))
            .collect();
        if !out.push(ImportRecord::new(Arc::clone(&headers), values)) {
            return Ok(());
        }
    }
}

fn stream_jsonl(reader: impl BufRead, out: &mut Batcher<'_, '_>) -> TransferResult<()> {
    let mut seen = false;
    for (i, line) in reader.lines().enumerate() {
        let line =
            line.map_err(|e| TransferError::InvalidConfig(format!("jsonl line {}: {e}", i + 1)))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: JsonValue = serde_json::from_str(line)
            .map_err(|e| TransferError::InvalidConfig(format!("jsonl line {}: {e}", i + 1)))?;
        let JsonValue::Object(object) = value else {
            return Err(TransferError::InvalidConfig(format!(
                "jsonl line {} is not an object",
                i + 1
            )));
        };
        seen = true;
        if !out.push(ImportRecord::from_object(object)) {
            return Ok(());
        }
    }
    if !seen {
        return Err(TransferError::InvalidConfig(
            "JSON Lines file is empty.".into(),
        ));
    }
    Ok(())
}

/// Sentinel error: the receiver asked to stop. Not a parse failure.
const STOP: &str = "\u{0}mystique-import-stop";

fn stream_json(reader: impl Read, out: &mut Batcher<'_, '_>) -> TransferResult<()> {
    let mut de = serde_json::Deserializer::from_reader(reader);
    let result = (&mut de).deserialize_any(TopVisitor { out });
    match result {
        Ok(()) => {}
        Err(err) if err.to_string().starts_with(STOP) => return Ok(()),
        Err(err) => return Err(TransferError::InvalidConfig(format!("json: {err}"))),
    }
    de.end()
        .map_err(|e| TransferError::InvalidConfig(format!("json: {e}")))
}

struct TopVisitor<'x, 'a, 'b> {
    out: &'x mut Batcher<'a, 'b>,
}

impl<'de> Visitor<'de> for TopVisitor<'_, '_, '_> {
    type Value = ();

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("an object, an array of objects, or an array of arrays")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut object = Map::new();
        while let Some((key, value)) = map.next_entry::<String, JsonValue>()? {
            object.insert(key, value);
        }
        self.out.push(ImportRecord::from_object(object));
        Ok(())
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let mut header: Option<Arc<[String]>> = None;
        let mut first = true;
        while let Some(item) = seq.next_element_seed(ItemSeed)? {
            let record = match item {
                JsonValue::Object(object) => ImportRecord::from_object(object),
                JsonValue::Array(cells) => {
                    if first && cells.iter().all(JsonValue::is_string) && !cells.is_empty() {
                        header = Some(
                            cells
                                .iter()
                                .enumerate()
                                .map(|(i, c)| {
                                    c.as_str()
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .unwrap_or_else(|| format!("col_{}", i + 1))
                                })
                                .collect::<Vec<_>>()
                                .into(),
                        );
                        first = false;
                        continue;
                    }
                    let keys: Arc<[String]> = match &header {
                        Some(keys) => Arc::clone(keys),
                        None => (0..cells.len())
                            .map(|i| format!("col_{}", i + 1))
                            .collect::<Vec<_>>()
                            .into(),
                    };
                    ImportRecord::new(keys, cells)
                }
                _ => {
                    return Err(de::Error::custom(
                        "JSON must be an object, an array of objects, or an array of arrays.",
                    ))
                }
            };
            first = false;
            if !self.out.push(record) {
                return Err(de::Error::custom(STOP));
            }
        }
        Ok(())
    }
}

struct ItemSeed;

impl<'de> DeserializeSeed<'de> for ItemSeed {
    type Value = JsonValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<JsonValue, D::Error> {
        serde::Deserialize::deserialize(deserializer)
    }
}

/// Decodes `encoding` to UTF-8 as it reads. BOMs are honoured, invalid
/// sequences become U+FFFD (same as reading the whole file).
struct DecodeReader<R> {
    inner: R,
    decoder: encoding_rs::Decoder,
    raw: Vec<u8>,
    decoded: Vec<u8>,
    pos: usize,
    eof: bool,
}

impl<R: Read> DecodeReader<R> {
    fn new(inner: R, encoding: ImportEncoding) -> Self {
        Self {
            inner,
            decoder: encoding.decoder().new_decoder(),
            raw: vec![0; 64 * 1024],
            decoded: Vec::new(),
            pos: 0,
            eof: false,
        }
    }

    fn fill(&mut self) -> std::io::Result<()> {
        self.decoded.clear();
        self.pos = 0;
        while self.decoded.is_empty() && !self.eof {
            let n = self.inner.read(&mut self.raw)?;
            let last = n == 0;
            let room = self.decoder.max_utf8_buffer_length(n).unwrap_or(n * 3 + 16);
            self.decoded.resize(room, 0);
            let (_, _, written, _) =
                self.decoder
                    .decode_to_utf8(&self.raw[..n], &mut self.decoded, last);
            self.decoded.truncate(written);
            self.eof = last;
        }
        Ok(())
    }
}

impl<R: Read> Read for DecodeReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.decoded.len() {
            self.fill()?;
            if self.decoded.is_empty() {
                return Ok(0);
            }
        }
        let n = buf.len().min(self.decoded.len() - self.pos);
        buf[..n].copy_from_slice(&self.decoded[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "mystique-stream-{}-{nanos}-{name}",
            std::process::id()
        ));
        std::fs::write(&path, bytes).expect("write");
        path
    }

    fn collect(
        path: &std::path::Path,
        format: ImportFormat,
        batch: usize,
    ) -> Vec<Vec<ImportRecord>> {
        let mut batches = Vec::new();
        stream_import_file(
            &path.to_string_lossy(),
            format,
            ImportEncoding::Utf8,
            batch,
            &mut |b| {
                batches.push(b);
                true
            },
        )
        .expect("stream");
        batches
    }

    #[test]
    fn csv_streams_in_batches() {
        let mut text = String::from("id,name\n");
        for i in 0..25 {
            text.push_str(&format!("{i},n{i}\n"));
        }
        let path = temp("a.csv", text.as_bytes());
        let batches = collect(&path, ImportFormat::Csv, 10);
        assert_eq!(
            batches.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![10, 10, 5]
        );
        assert_eq!(batches[2][4].get("id"), Some(&json!(24)));
        assert_eq!(batches[0][0].get("name"), Some(&json!("n0")));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn json_arrays_stream_element_by_element() {
        let path = temp("a.json", br#"[{"a":1},{"a":2,"b":"x"},{"b":"y"}]"#);
        let batches = collect(&path, ImportFormat::Json, 2);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0][1].get("b"), Some(&json!("x")));
        assert_eq!(batches[1][0].get("b"), Some(&json!("y")));
        let _ = std::fs::remove_file(&path);

        let path = temp("b.json", br#"[["id","name"],[1,"a"],[2]]"#);
        let batches = collect(&path, ImportFormat::Json, 10);
        assert_eq!(batches[0].len(), 2);
        assert_eq!(batches[0][1].get("name"), Some(&JsonValue::Null));
        let _ = std::fs::remove_file(&path);

        let path = temp("c.json", br#"{"id":7}"#);
        assert_eq!(
            collect(&path, ImportFormat::Json, 10)[0][0].get("id"),
            Some(&json!(7))
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stopping_early_reads_no_further() {
        let mut text = String::from("[");
        for i in 0..1000 {
            if i > 0 {
                text.push(',');
            }
            text.push_str(&format!("{{\"i\":{i}}}"));
        }
        text.push_str("] trailing garbage that is never reached");
        let path = temp("stop.json", text.as_bytes());
        let mut seen = 0;
        stream_import_file(
            &path.to_string_lossy(),
            ImportFormat::Json,
            ImportEncoding::Utf8,
            100,
            &mut |b| {
                seen += b.len();
                false
            },
        )
        .expect("stopped cleanly");
        assert_eq!(seen, 100);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn utf16_with_a_bom_decodes_as_it_streams() {
        let text = "id,name\n1,café\n";
        let mut bytes = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let path = temp("u16.csv", &bytes);
        let mut rows = Vec::new();
        stream_import_file(
            &path.to_string_lossy(),
            ImportFormat::Csv,
            ImportEncoding::Utf16Le,
            10,
            &mut |b| {
                rows.extend(b);
                true
            },
        )
        .expect("utf16");
        assert_eq!(rows[0].get("name"), Some(&json!("café")));
        let _ = std::fs::remove_file(&path);
    }
}
