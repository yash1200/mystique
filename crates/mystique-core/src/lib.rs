//! # mystique-core
//!
//! Domain logic and local persistence for MystiqueDB.
//!
//! ## Layers
//!
//! ```text
//! domain/          Pure types (no I/O)
//! storage/         Persistence adapters (sqlite today; keychain later)
//! application/     Use-case services (profiles, query library, …)
//! ```
//!
//! ## Future
//!
//! - `connectors/` or separate crates: `mystique-plugin-postgres`, …
//! - Services orchestrate connectors + storage without knowing Tauri.
//!
//! The Tauri shell (`src-tauri`) only wires IPC → `AppServices`.

pub mod application;
pub mod domain;
pub mod error;
pub mod storage;

pub use application::AppServices;
pub use domain::*;
pub use error::{AppError, AppResult};
pub use storage::AppDatabase;
