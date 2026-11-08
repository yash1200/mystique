//! Cooperative cancel for one running statement.
//!
//! Core owns the deadline and the registry. Plugins register an engine
//! interrupt (Postgres cancel, `KILL QUERY`, SQLite `InterruptHandle`) and
//! keep polling the driver future after that interrupt so `RESET` / `ROLLBACK`
//! still run. Dropping the driver future first skips that cleanup.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::Notify;

use crate::error::{ConnectorError, ConnectorResult};

/// How long core keeps polling a plugin after cancel so it can roll back.
pub const CANCEL_CLEANUP_MS: u64 = 2_000;

/// No further row for this long, after the first row, is a stall.
///
/// The first row stays on the caller's overall deadline. `pg_sleep` / `SLEEP`
/// are allowed to use that deadline. A later gap with no bytes is not.
pub const INACTIVITY_TIMEOUT_MS: u64 = 30_000;

/// The stall limit for a run with this timeout. A run with no timeout
/// ([`crate::NO_TIMEOUT`]) has no stall limit either.
pub fn inactivity_limit(timeout_ms: u64) -> Option<Duration> {
    (timeout_ms != crate::NO_TIMEOUT).then(|| Duration::from_millis(INACTIVITY_TIMEOUT_MS))
}

/// Fires when [`CancellationToken::cancel`] runs.
///
/// Built on [`Notify`] so connector-api does not depend on `tokio-util`.
/// Constructing one does not need a runtime.
#[derive(Debug)]
pub struct CancellationToken {
    cancelled: AtomicBool,
    notify: Notify,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Resolves once [`cancel`](Self::cancel) has been called.
    ///
    /// The waiter is enabled before the flag is read again, so a cancel that
    /// lands between the subscribe and the check is not lost.
    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        // `Notified` only subscribes when polled or enabled. `pin!` keeps the
        // waiter alive for the await; a temporary `Pin::new` would dangle.
        let notified = self.notify.notified();
        let mut notified = std::pin::pin!(notified);
        notified.as_mut().enable();
        if self.is_cancelled() {
            return;
        }
        notified.await;
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// One in-flight statement: a token plus the engine's sync interrupt hook.
///
/// The hook is `FnOnce` and sync because SQLite's interrupt runs on whatever
/// thread cancelled. Plugins that need async (`KILL QUERY`, Postgres cancel)
/// spawn from the hook.
pub struct QueryControl {
    pub token: CancellationToken,
    interrupt: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl std::fmt::Debug for QueryControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueryControl")
            .field("cancelled", &self.token.is_cancelled())
            .finish()
    }
}

impl QueryControl {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            token: CancellationToken::new(),
            interrupt: Mutex::new(None),
        })
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    fn interrupt_slot(&self) -> MutexGuard<'_, Option<Box<dyn FnOnce() + Send>>> {
        self.interrupt.lock().unwrap_or_else(|err| err.into_inner())
    }

    /// Register the engine hook.
    ///
    /// Replaces any previous hook. The latest connection is the one to kill
    /// (export pages checkout a new connection per chunk). If the token is
    /// already cancelled, the hook runs on this thread before this returns.
    pub fn set_interrupt(&self, hook: impl FnOnce() + Send + 'static) {
        let mut slot = self.interrupt_slot();
        if self.token.is_cancelled() {
            drop(slot);
            hook();
            return;
        }
        *slot = Some(Box::new(hook));
    }

    /// Drop a hook that must not outlive its connection.
    pub fn clear_interrupt(&self) {
        let _ = self.interrupt_slot().take();
    }

    /// Mark cancelled, then run the hook once.
    ///
    /// The flag is set while the mutex is held so [`set_interrupt`] cannot
    /// store a hook that this call already missed.
    pub fn cancel(&self) {
        let hook = {
            let mut slot = self.interrupt_slot();
            self.token.cancel();
            slot.take()
        };
        if let Some(hook) = hook {
            hook();
        }
    }
}

/// Clears the interrupt hook when the connection is about to return to the pool.
pub struct InterruptGuard<'a> {
    control: &'a QueryControl,
}

impl<'a> InterruptGuard<'a> {
    pub fn new(control: &'a QueryControl) -> Self {
        Self { control }
    }
}

impl Drop for InterruptGuard<'_> {
    fn drop(&mut self) {
        self.control.clear_interrupt();
    }
}

/// Why [`poll_until_idle`] stopped before the inner future produced a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleStop {
    Cancelled,
    Inactive,
}

impl IdleStop {
    pub fn into_error(self) -> ConnectorError {
        match self {
            Self::Cancelled => ConnectorError::Query("Query cancelled.".into()),
            Self::Inactive => {
                ConnectorError::Timeout(format!("No rows arrived for {INACTIVITY_TIMEOUT_MS} ms."))
            }
        }
    }
}

/// Poll `work` until it finishes, the token fires, or `inactivity` elapses.
/// `None` waits for the work or the token only.
///
/// On cancel or inactivity the inner future is polled for
/// [`CANCEL_CLEANUP_MS`] after the interrupt, not dropped. Drivers need that
/// window to finish `RESET` / `ROLLBACK`. Mongo is the exception: dropping the
/// cursor is what aborts the command, so that plugin does not use this helper.
pub async fn poll_until_idle<T>(
    control: &QueryControl,
    inactivity: Option<Duration>,
    work: impl Future<Output = T>,
) -> Result<T, IdleStop> {
    tokio::pin!(work);
    let cleanup = Duration::from_millis(CANCEL_CLEANUP_MS);
    tokio::select! {
        biased;
        value = &mut work => Ok(value),
        _ = control.token.cancelled() => {
            let _ = tokio::time::timeout(cleanup, &mut work).await;
            Err(IdleStop::Cancelled)
        }
        _ = tokio::time::sleep(inactivity.unwrap_or_default()), if inactivity.is_some() => {
            control.cancel();
            let _ = tokio::time::timeout(cleanup, &mut work).await;
            Err(IdleStop::Inactive)
        }
    }
}

/// Map an idle stop into the connector error the grid already renders.
pub fn idle_result<T>(result: Result<T, IdleStop>) -> ConnectorResult<T> {
    result.map_err(IdleStop::into_error)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;

    #[test]
    fn cancel_runs_the_hook_once() {
        let control = QueryControl::new();
        let hits = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&hits);
        control.set_interrupt(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        });
        control.cancel();
        control.cancel();
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert!(control.is_cancelled());
    }

    #[test]
    fn set_interrupt_after_cancel_runs_immediately() {
        let control = QueryControl::new();
        control.cancel();
        let hits = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&hits);
        control.set_interrupt(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn clear_interrupt_drops_the_hook() {
        let control = QueryControl::new();
        let hits = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&hits);
        control.set_interrupt(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        });
        control.clear_interrupt();
        control.cancel();
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        assert!(control.is_cancelled());
    }

    #[tokio::test]
    async fn cancel_wakes_a_waiter() {
        let control = QueryControl::new();
        let waiting = Arc::clone(&control);
        let task = tokio::spawn(async move {
            waiting.token.cancelled().await;
        });
        tokio::task::yield_now().await;
        control.cancel();
        let joined = tokio::time::timeout(Duration::from_secs(1), task).await;
        assert!(joined.is_ok(), "waiter did not wake");
    }

    #[tokio::test]
    async fn already_cancelled_wait_returns() {
        let control = QueryControl::new();
        control.cancel();
        let joined = tokio::time::timeout(Duration::from_secs(1), control.token.cancelled()).await;
        assert!(joined.is_ok());
    }

    #[tokio::test]
    async fn inactivity_cancels_and_keeps_polling() {
        let control = QueryControl::new();
        let finished = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&finished);
        let watched = Arc::clone(&control);
        let work = async move {
            watched.token.cancelled().await;
            flag.store(true, Ordering::SeqCst);
        };
        let stop = poll_until_idle(&control, Some(Duration::from_millis(20)), work).await;
        assert_eq!(stop, Err(IdleStop::Inactive));
        assert!(finished.load(Ordering::SeqCst));
        assert!(control.is_cancelled());
    }

    #[tokio::test]
    async fn no_timeout_has_no_stall_limit() {
        assert_eq!(inactivity_limit(crate::NO_TIMEOUT), None);
        assert_eq!(
            inactivity_limit(5_000),
            Some(Duration::from_millis(INACTIVITY_TIMEOUT_MS))
        );
        let control = QueryControl::new();
        let work = async {
            tokio::time::sleep(Duration::from_millis(60)).await;
            7
        };
        let done = poll_until_idle(&control, inactivity_limit(crate::NO_TIMEOUT), work).await;
        assert_eq!(done, Ok(7));
        assert!(!control.is_cancelled());
    }
}
