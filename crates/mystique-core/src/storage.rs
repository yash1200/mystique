//! Persistence adapters.
//!
//! Today: SQLite under `sqlite`.
//! Later: keychain, file export, remote sync as sibling modules.

pub mod sqlite;

pub use sqlite::{db_file_name, AppDatabase};
