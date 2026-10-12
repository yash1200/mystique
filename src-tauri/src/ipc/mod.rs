pub mod agent;
pub mod engines;
pub mod history;
pub mod mcp;
pub mod profiles;
pub mod query_library;
pub mod settings;
pub mod workspace_tabs;

/// Run a sync IPC body. A panic becomes `Err("internal error: …")` so one
/// plugin bug does not abort the desktop process (`panic = "unwind"`).
pub fn guard_sync<T>(body: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(result) => result,
        Err(payload) => Err(format!("internal error: {}", panic_payload(&payload))),
    }
}

/// Run an async IPC body. Polled in place — Tauri `State` is not `'static`,
/// so the command future cannot be moved onto `tokio::spawn`.
pub async fn guard<T>(
    body: impl std::future::Future<Output = Result<T, String>> + Send,
) -> Result<T, String> {
    match futures_util::FutureExt::catch_unwind(std::panic::AssertUnwindSafe(body)).await {
        Ok(result) => result,
        Err(payload) => Err(format!("internal error: {}", panic_payload(&payload))),
    }
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

    #[test]
    fn guard_sync_returns_internal_error_when_the_body_panics() {
        let err = guard_sync(|| -> Result<(), String> { panic!("be-02 slice") }).unwrap_err();
        assert!(err.starts_with("internal error: "), "{err}");
        assert!(err.contains("be-02 slice"), "{err}");
    }

    #[test]
    fn guard_sync_passes_a_result_through() {
        let value = guard_sync(|| Ok::<_, String>(7)).expect("ok");
        assert_eq!(value, 7);
    }

    #[test]
    fn guard_returns_internal_error_when_the_future_panics() {
        let err = futures_executor::block_on(guard(async {
            panic!("plugin bug");
            #[allow(unreachable_code)]
            Ok::<(), String>(())
        }))
        .unwrap_err();
        assert!(err.starts_with("internal error: "), "{err}");
        assert!(err.contains("plugin bug"), "{err}");
    }
}
