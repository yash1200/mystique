//! Server NOTICE / WARNING messages for the statement that raised them.
//!
//! tokio-postgres hands notices to whoever polls the connection, not to the
//! query future. deadpool's default connection task logs them and drops them.
//! [`NoticeConnect`] runs its own task that keeps each connection's notices in
//! a [`NoticeBuf`]. A run finds its buffer by the server's backend pid, which
//! the hub caches per pooled connection, and drains it after every statement.
//!
//! The task reads a notice before the messages that follow it, so a notice is
//! in the buffer before the statement that raised it returns.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use deadpool_postgres::StatementCache;
use futures_util::StreamExt;
use tokio::task::JoinHandle;
use tokio_postgres::tls::{MakeTlsConnect, TlsConnect};
use tokio_postgres::{AsyncMessage, Client, Socket};

/// Notices kept per statement. A loop of `RAISE NOTICE` cannot grow memory.
const MAX_NOTICES: usize = 200;

/// One connection's notice buffer. Collects only while a run captures.
#[derive(Default)]
pub(crate) struct NoticeBuf {
    capturing: AtomicBool,
    lines: Mutex<Vec<String>>,
    dropped: Mutex<usize>,
}

impl NoticeBuf {
    fn push(&self, notice: &tokio_postgres::error::DbError) {
        if !self.capturing.load(Ordering::SeqCst) {
            return;
        }
        let mut lines = lock(&self.lines);
        if lines.len() >= MAX_NOTICES {
            *lock(&self.dropped) += 1;
            return;
        }
        lines.push(format_notice(notice));
    }

    fn take(&self) -> Vec<String> {
        let mut lines = std::mem::take(&mut *lock(&self.lines));
        let dropped = std::mem::take(&mut *lock(&self.dropped));
        if dropped > 0 {
            lines.push(format!("{dropped} more notices were not kept."));
        }
        lines
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|err| err.into_inner())
}

/// `NOTICE: relation "t" does not exist, skipping`, plus the hint if any.
pub(crate) fn format_notice(notice: &tokio_postgres::error::DbError) -> String {
    let mut text = format!("{}: {}", notice.severity(), notice.message());
    if let Some(detail) = notice.detail() {
        text.push_str(&format!(" Detail: {detail}"));
    }
    if let Some(hint) = notice.hint() {
        text.push_str(&format!(" Hint: {hint}"));
    }
    text
}

/// Notice buffers of one pool, by backend pid, plus a pid cache per pooled
/// connection so a run asks the server for its pid once per connection.
#[derive(Clone, Default)]
pub(crate) struct NoticeHub {
    by_pid: Arc<Mutex<HashMap<i32, Weak<NoticeBuf>>>>,
    /// Keyed by the address of the connection's statement cache, which lives
    /// exactly as long as the pooled connection. The `Weak` proves the address
    /// was not reused by a newer connection.
    pids: Arc<Mutex<HashMap<usize, CachedPid>>>,
}

impl NoticeHub {
    fn register(&self, pid: i32, buf: &Arc<NoticeBuf>) {
        let mut map = lock(&self.by_pid);
        map.retain(|_, weak| weak.strong_count() > 0);
        map.insert(pid, Arc::downgrade(buf));
    }

    /// Start collecting notices on `client`. `None` when this connection has
    /// no buffer (the server has no `pg_backend_pid()`): the run still works.
    pub(crate) async fn capture(
        &self,
        client: &deadpool_postgres::Object,
    ) -> Option<NoticeCapture> {
        let pid = self.pid_of(client).await?;
        let buf = lock(&self.by_pid).get(&pid).and_then(Weak::upgrade)?;
        buf.take();
        buf.capturing.store(true, Ordering::SeqCst);
        Some(NoticeCapture { buf })
    }

    async fn pid_of(&self, client: &deadpool_postgres::Object) -> Option<i32> {
        let cache = &client.statement_cache;
        let key = Arc::as_ptr(cache) as usize;
        {
            let pids = lock(&self.pids);
            if let Some((weak, pid)) = pids.get(&key) {
                if weak.upgrade().is_some_and(|live| Arc::ptr_eq(&live, cache)) {
                    return Some(*pid);
                }
            }
        }
        let pid = backend_pid(client).await?;
        let mut pids = lock(&self.pids);
        pids.retain(|_, (weak, _)| weak.strong_count() > 0);
        pids.insert(key, (Arc::downgrade(cache), pid));
        Some(pid)
    }
}

async fn backend_pid(client: &Client) -> Option<i32> {
    let row = client
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .ok()?;
    row.try_get::<_, i32>(0).ok()
}

/// Collects one run's notices. Dropping it stops collecting.
pub(crate) struct NoticeCapture {
    buf: Arc<NoticeBuf>,
}

impl NoticeCapture {
    /// Notices since the last call.
    pub(crate) fn take(&self) -> Vec<String> {
        self.buf.take()
    }
}

impl Drop for NoticeCapture {
    fn drop(&mut self) {
        self.buf.capturing.store(false, Ordering::SeqCst);
        self.buf.take();
    }
}

/// Notices of an optional capture. Runs without one report none.
pub(crate) fn take_notices(capture: Option<&NoticeCapture>) -> Vec<String> {
    capture.map(NoticeCapture::take).unwrap_or_default()
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A pooled connection's statement cache, which proves the cache key still
/// names that connection, and its backend pid.
type CachedPid = (Weak<StatementCache>, i32);

/// deadpool connect that polls the connection itself to keep its notices.
pub(crate) struct NoticeConnect<T> {
    pub tls: T,
    pub hub: NoticeHub,
}

impl<T> deadpool_postgres::Connect for NoticeConnect<T>
where
    T: MakeTlsConnect<Socket> + Clone + Sync + Send + 'static,
    T::Stream: Sync + Send,
    T::TlsConnect: Sync + Send,
    <T::TlsConnect as TlsConnect<Socket>>::Future: Send,
{
    fn connect(
        &self,
        pg_config: &tokio_postgres::Config,
    ) -> BoxFuture<'_, Result<(Client, JoinHandle<()>), tokio_postgres::Error>> {
        let tls = self.tls.clone();
        let pg_config = pg_config.clone();
        let hub = self.hub.clone();
        Box::pin(async move {
            let (client, mut connection) = pg_config.connect(tls).await?;
            let buf = Arc::new(NoticeBuf::default());
            let task_buf = Arc::clone(&buf);
            let task = tokio::spawn(async move {
                let mut messages =
                    futures_util::stream::poll_fn(move |cx| connection.poll_message(cx));
                while let Some(message) = messages.next().await {
                    match message {
                        Ok(AsyncMessage::Notice(notice)) => task_buf.push(&notice),
                        Ok(_) => {}
                        // The client sees the error on its next call.
                        Err(_) => break,
                    }
                }
            });
            if let Some(pid) = backend_pid(&client).await {
                hub.register(pid, &buf);
            }
            Ok((client, task))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_drains_and_says_how_many_were_dropped() {
        // `DbError` has no public constructor; the live test covers `push`.
        let buf = NoticeBuf::default();
        {
            let mut lines = lock(&buf.lines);
            for index in 0..MAX_NOTICES {
                lines.push(format!("NOTICE: {index}"));
            }
        }
        *lock(&buf.dropped) += 3;
        let taken = buf.take();
        assert_eq!(taken.len(), MAX_NOTICES + 1);
        assert_eq!(
            taken.last().map(String::as_str),
            Some("3 more notices were not kept.")
        );
        assert!(buf.take().is_empty());
    }
}
