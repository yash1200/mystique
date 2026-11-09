//! Catalog metadata cache, per session (one connection) and database.
//!
//! Explorer listings, table descriptions, bulk columns and relationships are
//! read once and served from here until something may have changed them:
//!
//! - a statement that may be DDL ran on the pool or on a pinned tab, even if
//!   it failed (a failed multi-part DDL can still have done part of its work);
//! - a pinned tab committed or rolled back (its DDL becomes visible or goes);
//! - the structure editor, schema diff, table admin or an import ran;
//! - the user pressed refresh (`invalidate_metadata`);
//! - the session closed.
//!
//! Entries also expire after [`META_TTL`] so DDL from another client shows up
//! without a manual refresh eventually.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use mystique_connector_api::{CatalogColumnRow, CatalogDatabase, CatalogRelationship, ColumnInfo};

pub(super) const META_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_ENTRIES: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum MetaKey {
    Describe {
        database: Option<String>,
        schema: Option<String>,
        table: String,
    },
    Columns {
        database: Option<String>,
        schema: Option<String>,
    },
    Relationships {
        database: Option<String>,
    },
    Catalog {
        preferred: Option<String>,
        include_objects: bool,
    },
}

#[derive(Debug, Clone)]
pub(super) enum MetaValue {
    Describe(Vec<ColumnInfo>),
    Columns(Vec<CatalogColumnRow>),
    Relationships(Vec<CatalogRelationship>),
    Catalog(Vec<CatalogDatabase>),
}

struct Entry {
    at: Instant,
    value: MetaValue,
}

#[derive(Default)]
pub(crate) struct MetaCache {
    /// session id → entries.
    sessions: Mutex<HashMap<String, HashMap<MetaKey, Entry>>>,
}

impl MetaCache {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, HashMap<MetaKey, Entry>>> {
        self.sessions.lock().unwrap_or_else(|err| err.into_inner())
    }

    pub(super) fn get(&self, session_id: &str, key: &MetaKey) -> Option<MetaValue> {
        let guard = self.lock();
        let entry = guard.get(session_id)?.get(key)?;
        if entry.at.elapsed() >= META_TTL {
            return None;
        }
        Some(entry.value.clone())
    }

    pub(super) fn put(&self, session_id: &str, key: MetaKey, value: MetaValue) {
        let mut guard = self.lock();
        let entries = guard.entry(session_id.to_string()).or_default();
        if entries.len() >= MAX_ENTRIES {
            entries.retain(|_, entry| entry.at.elapsed() < META_TTL);
            if entries.len() >= MAX_ENTRIES {
                entries.clear();
            }
        }
        entries.insert(
            key,
            Entry {
                at: Instant::now(),
                value,
            },
        );
    }

    /// Drop everything cached for a session.
    pub(super) fn invalidate(&self, session_id: &str) {
        let dropped = self.lock().remove(session_id).map_or(0, |e| e.len());
        if dropped > 0 {
            tracing::debug!(session_id, dropped, "metadata cache invalidated");
        }
    }

    /// Drop one database's entries; entries not tied to a database go too.
    pub(super) fn invalidate_database(&self, session_id: &str, database: &str) {
        let mut guard = self.lock();
        let Some(entries) = guard.get_mut(session_id) else {
            return;
        };
        entries.retain(|key, _| match key {
            MetaKey::Describe { database: db, .. }
            | MetaKey::Columns { database: db, .. }
            | MetaKey::Relationships { database: db } => {
                db.as_deref().is_some_and(|db| db != database)
            }
            MetaKey::Catalog { .. } => false,
        });
    }

    #[cfg(test)]
    pub(super) fn len(&self, session_id: &str) -> usize {
        self.lock().get(session_id).map_or(0, |e| e.len())
    }
}

/// A statement that may change the catalog. Reads, `SET`, transaction
/// control and plain DML do not; anything else might (DDL, `DO` blocks,
/// procedure calls, vendor statements). Over-invalidating only costs a
/// re-read.
pub(super) fn may_change_catalog(sql: &str) -> bool {
    let first = first_word(sql);
    !matches!(
        first.as_str(),
        "" | "select"
            | "with"
            | "values"
            | "table"
            | "show"
            | "explain"
            | "describe"
            | "desc"
            | "pragma"
            | "insert"
            | "update"
            | "delete"
            | "merge"
            | "upsert"
            | "replace"
            | "set"
            | "reset"
            | "begin"
            | "start"
            | "commit"
            | "end"
            | "rollback"
            | "abort"
            | "savepoint"
            | "release"
            | "fetch"
            | "declare"
            | "close"
            | "move"
            | "copy"
            | "lock"
            | "use"
            | "load"
            | "handler"
            | "prepare"
            | "execute"
            | "deallocate"
            | "listen"
            | "notify"
            | "unlisten"
            | "discard"
            | "checkpoint"
    )
}

fn first_word(sql: &str) -> String {
    let mut rest = sql.trim_start();
    loop {
        if let Some(after) = rest.strip_prefix("--") {
            rest = after
                .split_once('\n')
                .map_or("", |(_, tail)| tail)
                .trim_start();
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after
                .split_once("*/")
                .map_or("", |(_, tail)| tail)
                .trim_start();
        } else {
            break;
        }
    }
    rest.trim_start_matches('(')
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ddl_and_unknown_statements_invalidate_and_dml_does_not() {
        for sql in [
            "CREATE TABLE t (id int)",
            "  alter table t add column x int",
            "DROP INDEX i",
            "-- note\nTRUNCATE t",
            "COMMENT ON TABLE t IS 'x'",
            "DO $$ BEGIN EXECUTE 'create table x()'; END $$",
            "CALL make_tables()",
            "RENAME TABLE a TO b",
            "GRANT SELECT ON t TO bob",
            "VACUUM",
        ] {
            assert!(may_change_catalog(sql), "{sql}");
        }
        for sql in [
            "SELECT 1",
            "/* x */ insert into t values (1)",
            "UPDATE t SET a = 1",
            "DELETE FROM t",
            "WITH x AS (SELECT 1) SELECT * FROM x",
            "BEGIN",
            "COMMIT",
            "SET search_path = public",
            "(SELECT 1)",
        ] {
            assert!(!may_change_catalog(sql), "{sql}");
        }
    }

    #[test]
    fn entries_are_per_session_and_database() {
        let cache = MetaCache::default();
        let describe = |db: &str| MetaKey::Describe {
            database: Some(db.into()),
            schema: None,
            table: "t".into(),
        };
        cache.put("a", describe("one"), MetaValue::Describe(Vec::new()));
        cache.put("a", describe("two"), MetaValue::Describe(Vec::new()));
        cache.put(
            "a",
            MetaKey::Catalog {
                preferred: None,
                include_objects: true,
            },
            MetaValue::Catalog(Vec::new()),
        );
        cache.put("b", describe("one"), MetaValue::Describe(Vec::new()));
        assert!(cache.get("a", &describe("one")).is_some());

        cache.invalidate_database("a", "one");
        assert!(cache.get("a", &describe("one")).is_none());
        assert!(cache.get("a", &describe("two")).is_some());
        assert_eq!(cache.len("a"), 1, "the explorer listing spans databases");

        cache.invalidate("a");
        assert_eq!(cache.len("a"), 0);
        assert!(cache.get("b", &describe("one")).is_some());
    }
}
