//! Large values: read one cell by key, page it from one snapshot, and attach
//! locators to clipped query-result cells.
//!
//! The first chunk (offset 0) reads the whole cell once with an index-friendly
//! key match (`col = $1`, typed by the engine) and keeps it here. Later chunks
//! are sliced from that copy, so the modal's chunks all come from one read
//! and each chunk costs its own length, not the whole value.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::ConnectionManager;
use mystique_connector_api::{
    cell_display_text, clip_query_result_with, large_cell_select, query_result_has_large,
    single_table_select, CollectSink, ColumnInfo, ColumnOrigin, ConnectorError, ConnectorResult,
    FetchLargeValueInput, FetchLargeValueRequest, FetchLargeValueResult, KeyColumn,
    LargeCellLocator, QueryBatchResult, QueryControl, ResolvedColumn, RowSource, SqlDialect,
    FETCH_TIMEOUT_MS, LARGE_CELL_CHUNK_CHARS, LARGE_VALUE_MAX_BYTES,
};

/// Values kept for paging. Small: a modal reads one value at a time.
const KEEP_VALUES: usize = 8;
/// Total text kept for paging.
const KEEP_BYTES: usize = 256 * 1024 * 1024;
const KEEP_FOR: Duration = Duration::from_secs(10 * 60);

/// `(database, schema, table)`.
type TableId = (Option<String>, Option<String>, String);

/// One value's display text plus a byte offset every
/// [`LARGE_CELL_CHUNK_CHARS`] characters, so a chunk is found without
/// walking the text from the start.
pub(crate) struct LargeText {
    text: String,
    marks: Vec<usize>,
    total_chars: u64,
}

impl LargeText {
    pub(crate) fn new(text: String) -> Self {
        let mut marks = vec![0];
        let mut total: u64 = 0;
        for (byte, _) in text.char_indices() {
            if total > 0 && total.is_multiple_of(LARGE_CELL_CHUNK_CHARS) {
                marks.push(byte);
            }
            total += 1;
        }
        Self {
            text,
            marks,
            total_chars: total,
        }
    }

    fn byte_at(&self, char_offset: u64) -> usize {
        let mark = (char_offset / LARGE_CELL_CHUNK_CHARS) as usize;
        let start = self.marks.get(mark).copied().unwrap_or(self.text.len());
        let skip = (char_offset % LARGE_CELL_CHUNK_CHARS) as usize;
        self.text[start..]
            .char_indices()
            .nth(skip)
            .map_or(self.text.len(), |(i, _)| start + i)
    }

    pub(crate) fn page(&self, offset: u64, limit: u64) -> FetchLargeValueResult {
        let start = offset.min(self.total_chars);
        let take = limit.clamp(1, LARGE_CELL_CHUNK_CHARS);
        let end = start.saturating_add(take).min(self.total_chars);
        let from = self.byte_at(start);
        let to = self.byte_at(end);
        FetchLargeValueResult {
            text: self.text[from..to].to_string(),
            offset: start,
            next_offset: (end < self.total_chars).then_some(end),
            total_chars: self.total_chars,
        }
    }
}

struct Kept {
    key: String,
    at: Instant,
    value: Arc<LargeText>,
}

/// Recently read values, newest last.
#[derive(Default)]
pub(crate) struct LargeValueCache {
    kept: Mutex<VecDeque<Kept>>,
}

impl LargeValueCache {
    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<Kept>> {
        self.kept.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn get(&self, key: &str) -> Option<Arc<LargeText>> {
        let kept = self.lock();
        kept.iter()
            .rev()
            .find(|k| k.key == key && k.at.elapsed() < KEEP_FOR)
            .map(|k| Arc::clone(&k.value))
    }

    fn put(&self, key: String, value: Arc<LargeText>) {
        let mut kept = self.lock();
        kept.retain(|k| k.key != key && k.at.elapsed() < KEEP_FOR);
        kept.push_back(Kept {
            key,
            at: Instant::now(),
            value,
        });
        let mut bytes: usize = kept.iter().map(|k| k.value.text.len()).sum();
        while kept.len() > KEEP_VALUES || (bytes > KEEP_BYTES && kept.len() > 1) {
            if let Some(old) = kept.pop_front() {
                bytes = bytes.saturating_sub(old.value.text.len());
            }
        }
    }

    /// A save or a closed session makes kept values stale.
    pub(crate) fn forget_session(&self, session_id: &str) {
        let prefix = format!("{session_id}\u{1}");
        self.lock().retain(|k| !k.key.starts_with(&prefix));
    }
}

fn cache_key(session_id: &str, tab_id: Option<&str>, locator: &LargeCellLocator) -> String {
    let locator = serde_json::to_string(locator).unwrap_or_default();
    format!("{session_id}\u{1}{}\u{1}{locator}", tab_id.unwrap_or(""))
}

fn too_large(bytes: usize) -> ConnectorError {
    ConnectorError::Query(format!(
        "This value is {} MB. Values over {} MB cannot be loaded here.",
        bytes.div_ceil(1024 * 1024),
        LARGE_VALUE_MAX_BYTES / (1024 * 1024)
    ))
}

impl ConnectionManager {
    /// One chunk of a clipped cell. Offset 0 reads the value; later offsets
    /// page the copy that read kept.
    pub async fn fetch_large_value(
        &self,
        req: FetchLargeValueRequest,
    ) -> ConnectorResult<FetchLargeValueResult> {
        let run = self.begin_run(&req.session_id, req.execution_id.clone());
        let control = Arc::clone(&run.control);
        let tab_id = req
            .tab_id
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty());
        let key = cache_key(&req.session_id, tab_id, &req.locator);
        let limit = req.chunk_limit();
        if req.offset > 0 {
            if let Some(kept) = self.large_values.get(&key) {
                return Ok(kept.page(req.offset, limit));
            }
        }
        let text = self
            .drive_registered(
                &control,
                FETCH_TIMEOUT_MS,
                self.read_large_text(&req.session_id, tab_id, &req.locator, &control),
            )
            .await?;
        let page = text.page(req.offset, limit);
        if page.next_offset.is_some() {
            self.large_values.put(key, text);
        }
        Ok(page)
    }

    /// The whole display text of one cell, refused past [`LARGE_VALUE_MAX_BYTES`].
    pub(crate) async fn read_large_text(
        &self,
        session_id: &str,
        tab_id: Option<&str>,
        locator: &LargeCellLocator,
        control: &Arc<QueryControl>,
    ) -> ConnectorResult<Arc<LargeText>> {
        let session = self.session(session_id).await?;
        let dialect = SqlDialect::from_engine(session.engine_id());
        let text = if dialect == SqlDialect::Generic {
            self.read_large_by_chunks(session_id, locator, control)
                .await?
        } else {
            let select = large_cell_select(dialect, locator)?;
            let source = RowSource::Select {
                sql: select.sql,
                params: select.params,
            };
            let mut sink = CollectSink::new(1);
            match self.pinned_for_read(session_id, tab_id).await {
                Some(pin) => pin.stream_rows(&source, control, &mut sink).await?,
                None => session.stream_rows(&source, control, &mut sink).await?,
            }
            let Some(cell) = sink
                .rows
                .into_iter()
                .next()
                .and_then(|r| r.into_iter().next())
            else {
                return Err(ConnectorError::Query(
                    "No row matched that key. It may have been changed or deleted.".into(),
                ));
            };
            cell_display_text(&cell)
        };
        if text.len() > LARGE_VALUE_MAX_BYTES {
            return Err(too_large(text.len()));
        }
        Ok(Arc::new(LargeText::new(text)))
    }

    /// Engines without a SQL read (Mongo) page through the plugin.
    async fn read_large_by_chunks(
        &self,
        session_id: &str,
        locator: &LargeCellLocator,
        control: &Arc<QueryControl>,
    ) -> ConnectorResult<String> {
        let session = self.session(session_id).await?;
        let mut text = String::new();
        let mut offset = 0;
        loop {
            let page = session
                .fetch_large_value(&FetchLargeValueInput {
                    locator: locator.clone(),
                    offset,
                    limit: LARGE_CELL_CHUNK_CHARS,
                    control: Arc::clone(control),
                })
                .await?;
            text.push_str(&page.text);
            if text.len() > LARGE_VALUE_MAX_BYTES {
                return Err(too_large(text.len()));
            }
            match page.next_offset {
                Some(next) if next > offset => offset = next,
                _ => return Ok(text),
            }
        }
    }

    /// Clip a batch for the grid. A clipped cell gets a locator when its
    /// column's source table is known and that table's key columns are all
    /// in the same row. `text` is the submitted batch (for SQLite, whose
    /// driver does not report column sources, the statement is parsed).
    pub async fn clip_batch_for(&self, session_id: &str, text: &str, batch: &mut QueryBatchResult) {
        for stmt in &mut batch.results {
            let Some(result) = stmt.result.as_mut() else {
                continue;
            };
            if !query_result_has_large(result) {
                continue;
            }
            let mut origins = std::mem::take(&mut stmt.column_origins);
            if origins.iter().all(Option::is_none) {
                let start = stmt.sql_range.start as usize;
                let end = (stmt.sql_range.end as usize).min(text.len());
                let sql = text.get(start..end).unwrap_or("");
                origins = self
                    .parsed_origins(session_id, sql, result.columns.len())
                    .await;
            }
            let resolved = self.resolve_origins(session_id, origins).await;
            clip_query_result_with(result, &resolved);
        }
    }

    /// Single-table SELECT only: map each output column to a table column.
    async fn parsed_origins(
        &self,
        session_id: &str,
        sql: &str,
        width: usize,
    ) -> Vec<Option<ColumnOrigin>> {
        let none = vec![None; width];
        let Ok(session) = self.session(session_id).await else {
            return none;
        };
        let dialect = SqlDialect::from_engine(session.engine_id());
        if dialect == SqlDialect::Generic {
            return none;
        }
        let Some(select) = single_table_select(sql, dialect) else {
            return none;
        };
        let (database, schema) = match dialect {
            SqlDialect::Mysql => (select.qualifier.clone(), None),
            SqlDialect::Sqlite => {
                // Only the main database: an attached one is not described here.
                if select
                    .qualifier
                    .as_deref()
                    .is_some_and(|q| !q.eq_ignore_ascii_case("main"))
                {
                    return none;
                }
                (None, None)
            }
            _ => (None, select.qualifier.clone()),
        };
        let Ok(columns) = self
            .describe_cached(
                session_id,
                database.as_deref(),
                schema.as_deref(),
                &select.table,
            )
            .await
        else {
            return none;
        };
        let mut out = Vec::with_capacity(width);
        for item in &select.items {
            match item {
                mystique_connector_api::ProjectedItem::Wildcard => {
                    out.extend(columns.iter().map(|c| {
                        Some(ColumnOrigin::named(
                            database.clone(),
                            schema.clone(),
                            select.table.clone(),
                            c.name.clone(),
                        ))
                    }));
                }
                mystique_connector_api::ProjectedItem::Column(name) => {
                    let found = columns.iter().find(|c| c.name.eq_ignore_ascii_case(name));
                    out.push(found.map(|c| {
                        ColumnOrigin::named(
                            database.clone(),
                            schema.clone(),
                            select.table.clone(),
                            c.name.clone(),
                        )
                    }));
                }
                mystique_connector_api::ProjectedItem::Other => out.push(None),
            }
        }
        // A count mismatch means the parse and the engine disagree. Trust neither.
        if out.len() != width {
            return none;
        }
        out
    }

    /// Names (through the plugin) and keys (through the cached describe).
    async fn resolve_origins(
        &self,
        session_id: &str,
        origins: Vec<Option<ColumnOrigin>>,
    ) -> Vec<Option<ResolvedColumn>> {
        let width = origins.len();
        if origins.iter().all(Option::is_none) {
            return vec![None; width];
        }
        let Ok(session) = self.session(session_id).await else {
            return vec![None; width];
        };
        let named = match session.name_column_origins(origins).await {
            Ok(named) => named,
            Err(err) => {
                tracing::debug!(error = %err, "could not name result columns");
                return vec![None; width];
            }
        };
        let mut described: Vec<(TableId, Vec<ColumnInfo>)> = Vec::new();
        let mut out = Vec::with_capacity(width);
        for origin in named {
            let Some(origin) = origin.filter(ColumnOrigin::has_names) else {
                out.push(None);
                continue;
            };
            let (Some(table), Some(column)) = (origin.table.clone(), origin.column.clone()) else {
                out.push(None);
                continue;
            };
            let id = (
                origin.database.clone(),
                origin.schema.clone(),
                table.clone(),
            );
            let columns = match described.iter().find(|(key, _)| *key == id) {
                Some((_, columns)) => columns.clone(),
                None => {
                    let columns = self
                        .describe_cached(
                            session_id,
                            origin.database.as_deref(),
                            origin.schema.as_deref(),
                            &table,
                        )
                        .await
                        .unwrap_or_default();
                    described.push((id, columns.clone()));
                    columns
                }
            };
            let Some(info) = columns.iter().find(|c| c.name == column) else {
                out.push(None);
                continue;
            };
            out.push(Some(ResolvedColumn {
                database: origin.database.clone(),
                schema: origin.schema.clone(),
                table,
                column,
                data_type: Some(info.data_type.clone()),
                table_key: columns
                    .iter()
                    .filter(|c| c.pk)
                    .map(|c| KeyColumn {
                        name: c.name.clone(),
                        data_type: Some(c.data_type.clone()),
                    })
                    .collect(),
            }));
        }
        out
    }
}

/// A read-only internal statement, planned like a typed one so the same
/// gates apply. Used by tests to check the large-value SQL is a plain read.
#[cfg(test)]
pub(super) fn plan_is_read(sql: &str, dialect: SqlDialect, params: &[serde_json::Value]) -> bool {
    mystique_connector_api::gate_sql_batch_in(
        sql,
        dialect,
        mystique_connector_api::QueryMode::Read,
        false,
        params,
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_come_from_one_copy_at_char_boundaries() {
        let mut text = String::new();
        for i in 0..100_000 {
            text.push(if i % 3 == 0 { 'é' } else { 'a' });
        }
        text.push_str("END");
        let value = LargeText::new(text.clone());
        assert_eq!(value.total_chars, 100_003);
        let mut offset = 0;
        let mut whole = String::new();
        loop {
            let page = value.page(offset, LARGE_CELL_CHUNK_CHARS);
            whole.push_str(&page.text);
            match page.next_offset {
                Some(next) => offset = next,
                None => break,
            }
        }
        assert_eq!(whole, text);
        // An offset inside a checkpoint window.
        let mid = value.page(32_005, 3);
        assert_eq!(
            mid.text,
            text.chars().skip(32_005).take(3).collect::<String>()
        );
    }

    #[test]
    fn the_cache_keeps_a_few_recent_values_per_session() {
        let cache = LargeValueCache::default();
        for i in 0..(KEEP_VALUES + 3) {
            cache.put(
                format!("s\u{1}\u{1}{i}"),
                Arc::new(LargeText::new("x".into())),
            );
        }
        assert!(cache.get("s\u{1}\u{1}0").is_none());
        assert!(cache
            .get(&format!("s\u{1}\u{1}{}", KEEP_VALUES + 2))
            .is_some());
        cache.put("t\u{1}\u{1}a".into(), Arc::new(LargeText::new("y".into())));
        cache.forget_session("s");
        assert!(cache
            .get(&format!("s\u{1}\u{1}{}", KEEP_VALUES + 2))
            .is_none());
        assert!(cache.get("t\u{1}\u{1}a").is_some());
    }

    #[test]
    fn the_key_read_is_a_plain_bound_read() {
        let locator = LargeCellLocator {
            database: None,
            schema: Some("public".into()),
            table: "docs".into(),
            column: "body".into(),
            keys: vec![mystique_connector_api::LargeCellKey {
                column: "id".into(),
                value: serde_json::json!("9007199254740993"),
                data_type: Some("int8".into()),
            }],
        };
        for dialect in [SqlDialect::Postgres, SqlDialect::Mysql, SqlDialect::Sqlite] {
            let select = large_cell_select(dialect, &locator).expect("select");
            assert!(
                plan_is_read(&select.sql, dialect, &select.params),
                "{dialect:?}"
            );
        }
    }
}
