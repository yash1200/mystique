//! Pure domain types and DTOs. No SQLite / Tauri dependencies.
//!
//! Future engine plugins should depend on types here (or a thinner
//! `mystique-connector-api` crate extracted from this module).

mod agent_chat;
mod connection;
mod grid_layout;
mod history;
pub(crate) mod host_identity;
pub(crate) mod mongo_uri;
mod query_library;
mod settings;
mod ssh;
mod time;
mod tool_access;
mod workspace_tabs;

pub use agent_chat::*;
pub use connection::*;
pub use grid_layout::*;
pub use history::*;
pub use query_library::*;
pub use settings::*;
pub use ssh::*;
pub(crate) use time::now_iso;
pub use tool_access::*;
pub use workspace_tabs::*;
