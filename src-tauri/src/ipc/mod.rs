//! Tauri IPC adapters — map invoke calls to `mystique_core` services.
//! Keep these thin: no SQL, no business rules.

pub mod history;
pub mod profiles;
pub mod query_library;
pub mod settings;
