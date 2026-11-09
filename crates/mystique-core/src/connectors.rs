//! Engine registry and live session management.
//!
//! Core never imports engine drivers — only [`mystique_connector_api`]
//! and plugin entry points registered at bootstrap.

mod manager;
mod registry;
mod ssh;

pub use manager::{
    ConnectionManager, OpenSessionRequest, PreviewWriteRowsRequest, TimedUnlockStatus,
    WriteRowPreview, ROW_PREVIEW_LIMIT, TIMED_UNLOCK_CHOICES_SECS,
};
pub(crate) use manager::{GridSaveStore, QueryHistoryWriter, SessionOrigin};
pub use registry::ConnectorRegistry;
