//! Persistence adapters.
//!
//! Today: SQLite under `sqlite`. Connection passwords: `crate::secrets`.

pub mod sqlite;

pub use sqlite::{db_file_name, AppDatabase};
