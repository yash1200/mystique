//! Pure domain types and DTOs. No SQLite / Tauri dependencies.
//!
//! Future engine plugins should depend on types here (or a thinner
//! `mystique-connector-api` crate extracted from this module).

mod connection;
mod er_diagram;
mod history;
mod query_library;
mod settings;
mod time;
mod workspace_tabs;

pub use connection::*;
pub use er_diagram::*;
pub use history::*;
pub use query_library::*;
pub use settings::*;
pub(crate) use time::now_iso;
pub use workspace_tabs::*;
