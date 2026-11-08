//! Last tool results, so `cell_char_offset` pages without running the
//! statement again. An `INSERT … RETURNING` must not insert twice.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use mystique_connector_api::QueryResult;
use serde_json::Value as JsonValue;

const TTL: Duration = Duration::from_secs(5 * 60);
const MAX_ENTRIES: usize = 8;
/// A larger result is not kept; a write then cannot be paged.
const MAX_ENTRY_BYTES: usize = 8 * 1024 * 1024;

/// One tool session (`agent:{id}` / `mcp:{id}`) plus the exact request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResultKey {
    pub session_id: String,
    pub query: String,
    pub database: Option<String>,
    pub collection: Option<String>,
    pub max_rows: u32,
}

struct Entry {
    key: ResultKey,
    at: Instant,
    result: QueryResult,
}

#[derive(Default)]
pub(crate) struct ToolResultCache {
    entries: Mutex<VecDeque<Entry>>,
}

impl ToolResultCache {
    pub fn get(&self, key: &ResultKey) -> Option<QueryResult> {
        let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        entries.retain(|e| e.at.elapsed() < TTL);
        entries
            .iter()
            .find(|e| &e.key == key)
            .map(|e| e.result.clone())
    }

    pub fn put(&self, key: ResultKey, result: &QueryResult) {
        let mut entries = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        entries.retain(|e| e.at.elapsed() < TTL && e.key != key);
        if approx_bytes(result) > MAX_ENTRY_BYTES {
            return;
        }
        entries.push_back(Entry {
            key,
            at: Instant::now(),
            result: result.clone(),
        });
        while entries.len() > MAX_ENTRIES {
            entries.pop_front();
        }
    }
}

fn approx_bytes(result: &QueryResult) -> usize {
    result
        .rows
        .iter()
        .flat_map(|row| row.iter())
        .map(value_bytes)
        .sum()
}

fn value_bytes(value: &JsonValue) -> usize {
    match value {
        JsonValue::String(s) => s.len(),
        JsonValue::Array(items) => items.iter().map(value_bytes).sum::<usize>() + 8,
        JsonValue::Object(map) => map.iter().map(|(k, v)| k.len() + value_bytes(v)).sum(),
        _ => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_connector_api::QueryMode;
    use serde_json::json;

    fn key(query: &str) -> ResultKey {
        ResultKey {
            session_id: "mcp:1".into(),
            query: query.into(),
            database: None,
            collection: None,
            max_rows: 200,
        }
    }

    fn result(cell: JsonValue) -> QueryResult {
        QueryResult {
            engine: "sqlite".into(),
            mode: QueryMode::Read,
            columns: Vec::new(),
            rows: vec![vec![cell]],
            row_count: 1,
            truncated: false,
            duration_ms: 0,
            message: None,
        }
    }

    #[test]
    fn keeps_the_newest_entries_and_skips_huge_results() {
        let cache = ToolResultCache::default();
        for i in 0..(MAX_ENTRIES + 2) {
            cache.put(key(&format!("q{i}")), &result(json!(i)));
        }
        assert!(cache.get(&key("q0")).is_none());
        assert!(cache.get(&key("q1")).is_none());
        assert_eq!(cache.get(&key("q2")).unwrap().rows[0][0], json!(2));
        assert!(cache.get(&key(&format!("q{}", MAX_ENTRIES + 1))).is_some());

        cache.put(key("huge"), &result(json!("x".repeat(MAX_ENTRY_BYTES + 1))));
        assert!(cache.get(&key("huge")).is_none());

        let mut other = key("q2");
        other.session_id = "agent:1".into();
        assert!(cache.get(&other).is_none());
    }
}
