//! IPC adapters.
//!
//! The one sync command left on the main thread touches neither the keychain
//! nor SQLite:
//! - `set_native_menu` — native menus are built on the UI thread
//!
//! Moved to `async` + [`blocking`] in 4.5.13 (keychain or metadata SQLite):
//! - profiles: `list_profiles`, `save_profile`, `delete_profile`,
//!   `touch_profile_connected`
//! - settings: `get_vault_status`, `retry_vault_unlock`, `purge_keychain_items`
//! - queries: `list_saved_queries`, `save_query`, `delete_saved_query`
//! - tabs: `list_workspace_tabs`, `replace_workspace_tabs`, `upsert_workspace_tab`
//! - history: `append_query_history`, `list_query_history`,
//!   `list_recent_query_history`
//! - agent: `get_ai_settings`, `set_ai_enabled`, `set_ai_ollama_url`,
//!   `set_ai_api_key`, `list_agent_chats`, `delete_agent_chat`,
//!   `list_agent_messages`, `update_agent_chat`
//! - mcp: `get_mcp_access`, `set_mcp_access`
//! - diagnostics: `export_diagnostics` (metadata SQLite + log files)
//! - engines: `trust_ssh_host_key` (known-hosts row). `test_engine_connection`
//!   and `open_engine_session` resolve the keychain password in [`blocking`]
//!   before the network `.await`
//!
//! `AppDatabase::with_conn` drops the SQLite mutex before it returns, so a
//! command does not hold that lock across `.await`. Commands that were already
//! `async` (engine queries, MCP status, agent send / model list) stay that way:
//! their metadata reads finish before the network `.await`. A metadata pool
//! is Phase 6.

pub mod agent;
pub mod diagnostics;
pub mod engines;
pub mod grid_layout;
pub mod history;
pub mod mcp;
pub mod profiles;
pub mod query_library;
pub mod settings;
pub mod workspace_tabs;

/// Every command rejects with this envelope (`kind`, `message`, optional
/// `position` / `prompt` / `transactionOutcome`). See
/// `mystique_connector_api::IpcError`.
pub use mystique_core::IpcError;

pub type IpcResult<T> = Result<T, IpcError>;

fn panicked(payload: &Box<dyn std::any::Any + Send>) -> IpcError {
    IpcError::internal(format!("internal error: {}", panic_payload(payload)))
}

/// Run a sync IPC body. A panic becomes an `internal` error so one plugin
/// bug does not abort the desktop process (`panic = "unwind"`).
pub fn guard_sync<T>(body: impl FnOnce() -> IpcResult<T>) -> IpcResult<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(result) => result,
        Err(payload) => Err(panicked(&payload)),
    }
}

/// Run an async IPC body. Polled in place — Tauri `State` is not `'static`,
/// so the command future cannot be moved onto `tokio::spawn`.
pub async fn guard<T>(
    body: impl std::future::Future<Output = IpcResult<T>> + Send,
) -> IpcResult<T> {
    match futures_util::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(body)).await {
        Ok(result) => result,
        Err(payload) => Err(panicked(&payload)),
    }
}

/// Keychain and metadata SQLite block the caller. Run them off the UI thread
/// and off the async worker. The closure is synchronous, so it cannot hold
/// the database mutex across an `.await`.
pub async fn blocking<T, F>(work: F) -> IpcResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> IpcResult<T> + Send + 'static,
{
    let joined = tauri::async_runtime::spawn_blocking(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
    })
    .await;
    match joined {
        Ok(Ok(result)) => result,
        Ok(Err(payload)) => Err(panicked(&payload)),
        Err(err) => Err(IpcError::internal(format!("internal error: {err}"))),
    }
}

/// [`guard`] around [`blocking`].
pub async fn guard_blocking<T, F>(work: F) -> IpcResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> IpcResult<T> + Send + 'static,
{
    guard(blocking(work)).await
}

fn panic_payload(payload: &Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "task panicked".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mystique_core::IpcErrorKind;

    #[test]
    fn guard_sync_returns_internal_error_when_the_body_panics() {
        let err = guard_sync(|| -> IpcResult<()> { panic!("be-02 slice") }).unwrap_err();
        assert_eq!(err.kind, IpcErrorKind::Internal);
        assert!(err.message.starts_with("internal error: "), "{err}");
        assert!(err.message.contains("be-02 slice"), "{err}");
    }

    #[test]
    fn guard_sync_passes_a_result_through() {
        let value = guard_sync(|| Ok(7)).expect("ok");
        assert_eq!(value, 7);
    }

    #[test]
    fn guard_returns_internal_error_when_the_future_panics() {
        let err = futures_executor::block_on(guard(async {
            panic!("plugin bug");
            #[allow(unreachable_code)]
            Ok::<(), IpcError>(())
        }))
        .unwrap_err();
        assert_eq!(err.kind, IpcErrorKind::Internal);
        assert!(err.message.contains("plugin bug"), "{err}");
    }

    #[test]
    fn a_connector_error_becomes_the_envelope() {
        let err = futures_executor::block_on(guard(async {
            Err::<(), _>(mystique_core::ConnectorError::Policy("Read-only.".into()).into())
        }))
        .unwrap_err();
        assert_eq!(err.kind, IpcErrorKind::Policy);
        assert_eq!(err.message, "Read-only.");
    }
}
