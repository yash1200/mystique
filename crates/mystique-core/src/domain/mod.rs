//! Pure domain types and DTOs. No SQLite / Tauri dependencies.
//!
//! Future engine plugins should depend on types here (or a thinner
//! `mystique-connector-api` crate extracted from this module).

mod connection;
mod history;
mod query_library;
mod settings;
mod time;

pub use connection::*;
pub use history::*;
pub use query_library::*;
pub use settings::*;
pub(crate) use time::now_iso;
