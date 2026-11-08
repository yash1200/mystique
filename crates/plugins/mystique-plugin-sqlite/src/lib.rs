//! SQLite engine plugin — file path based, r2d2 connection pool.

#![deny(clippy::indexing_slicing, clippy::unwrap_used)]

mod connector;
mod session;

pub use connector::SqliteConnector;

use std::sync::Arc;

use mystique_connector_api::DatabaseConnector;

pub fn connector() -> Arc<dyn DatabaseConnector> {
    Arc::new(SqliteConnector)
}

/// A temp database path no other test shares. A timestamp alone collides when
/// tests run in parallel; the counter keeps them apart, the timestamp keeps
/// a reused pid off an older run's leftover file.
#[cfg(test)]
pub(crate) fn temp_db_path(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "mystique-{tag}-{}-{nanos}-{n}.sqlite",
        std::process::id()
    ))
}
