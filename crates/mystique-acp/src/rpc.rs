//! JSON-RPC 2.0 over the agent's stdio: one JSON message per line.
//!
//! A reader task routes replies to the request that waits for them and
//! hands the agent's own requests and notifications to the session. A
//! writer task owns stdin. When the agent closes stdout, every waiting
//! request fails and the incoming queue ends.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value as JsonValue};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

/// A line longer than this ends the connection.
const MAX_LINE: usize = 32 * 1024 * 1024;

/// A JSON-RPC error, ours or the agent's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RpcError {
    pub code: i64,
    pub message: String,
}

impl RpcError {
    pub(crate) const CLOSED: i64 = -32099;

    fn closed() -> Self {
        Self {
            code: Self::CLOSED,
            message: "The agent closed the connection.".into(),
        }
    }
}

/// What the agent sent that is not a reply.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Incoming {
    Request {
        id: JsonValue,
        method: String,
        params: JsonValue,
    },
    Notification {
        method: String,
        params: JsonValue,
    },
}

type Waiting = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<JsonValue, RpcError>>>>>;

/// Our end of the connection. Cheap to share.
pub(crate) struct Peer {
    out: mpsc::UnboundedSender<String>,
    waiting: Waiting,
    next: AtomicU64,
}

/// The reader and writer tasks. Dropping this ends both.
pub(crate) struct Pumps {
    reader: JoinHandle<()>,
    writer: JoinHandle<()>,
}

impl Drop for Pumps {
    fn drop(&mut self) {
        self.reader.abort();
        self.writer.abort();
    }
}

pub(crate) fn connect<R, W>(
    reader: R,
    writer: W,
) -> (Arc<Peer>, mpsc::UnboundedReceiver<Incoming>, Pumps)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (out_tx, out_rx) = mpsc::unbounded_channel::<String>();
    let (in_tx, in_rx) = mpsc::unbounded_channel::<Incoming>();
    let waiting: Waiting = Arc::default();
    let peer = Arc::new(Peer {
        out: out_tx,
        waiting: Arc::clone(&waiting),
        next: AtomicU64::new(1),
    });
    let writer = tokio::spawn(write_lines(writer, out_rx));
    let reader = tokio::spawn(read_lines(reader, waiting, in_tx));
    (peer, in_rx, Pumps { reader, writer })
}

async fn write_lines<W: AsyncWrite + Unpin>(
    mut writer: W,
    mut rx: mpsc::UnboundedReceiver<String>,
) {
    while let Some(line) = rx.recv().await {
        if writer.write_all(line.as_bytes()).await.is_err()
            || writer.write_all(b"\n").await.is_err()
            || writer.flush().await.is_err()
        {
            break;
        }
    }
    // Closing stdin tells the agent we are gone.
    let _ = writer.shutdown().await;
}

async fn read_lines<R: AsyncRead + Unpin>(
    reader: R,
    waiting: Waiting,
    incoming: mpsc::UnboundedSender<Incoming>,
) {
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        line.clear();
        match read_line(&mut reader, &mut line).await {
            Ok(true) => {}
            Ok(false) => break,
            Err(reason) => {
                tracing::warn!(reason, "acp: closing the agent connection");
                break;
            }
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<JsonValue>(text) else {
            // Agents must not print anything else on stdout; skip it.
            tracing::debug!("acp: skipped a line that is not JSON");
            continue;
        };
        route(msg, &waiting, &incoming);
    }
    let pending: Vec<_> = waiting
        .lock()
        .map(|mut map| map.drain().map(|(_, tx)| tx).collect())
        .unwrap_or_default();
    for tx in pending {
        let _ = tx.send(Err(RpcError::closed()));
    }
}

/// One line into `buf`. `Ok(false)` at end of stream.
async fn read_line<R: AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    buf: &mut Vec<u8>,
) -> Result<bool, &'static str> {
    loop {
        let available = reader.fill_buf().await.map_err(|_| "read failed")?;
        if available.is_empty() {
            return Ok(!buf.is_empty());
        }
        let (take, done) = match available.iter().position(|b| *b == b'\n') {
            Some(at) => (at + 1, true),
            None => (available.len(), false),
        };
        buf.extend_from_slice(available.get(..take).unwrap_or_default());
        reader.consume(take);
        if buf.len() > MAX_LINE {
            return Err("message too large");
        }
        if done {
            return Ok(true);
        }
    }
}

fn route(msg: JsonValue, waiting: &Waiting, incoming: &mpsc::UnboundedSender<Incoming>) {
    let method = msg
        .get("method")
        .and_then(JsonValue::as_str)
        .map(str::to_owned);
    let id = msg.get("id").cloned().filter(|id| !id.is_null());
    match (method, id) {
        (Some(method), Some(id)) => {
            let params = msg.get("params").cloned().unwrap_or(JsonValue::Null);
            let _ = incoming.send(Incoming::Request { id, method, params });
        }
        (Some(method), None) => {
            let params = msg.get("params").cloned().unwrap_or(JsonValue::Null);
            let _ = incoming.send(Incoming::Notification { method, params });
        }
        (None, Some(id)) => {
            let Some(key) = id.as_u64() else { return };
            let Some(tx) = waiting.lock().ok().and_then(|mut map| map.remove(&key)) else {
                return;
            };
            let outcome = match msg.get("error") {
                Some(err) => Err(RpcError {
                    code: err
                        .get("code")
                        .and_then(JsonValue::as_i64)
                        .unwrap_or(-32603),
                    message: err
                        .get("message")
                        .and_then(JsonValue::as_str)
                        .unwrap_or("error")
                        .to_string(),
                }),
                None => Ok(msg.get("result").cloned().unwrap_or(JsonValue::Null)),
            };
            let _ = tx.send(outcome);
        }
        (None, None) => {}
    }
}

impl Peer {
    /// Send a request. Its reply arrives on the returned receiver, which can
    /// be awaited across several waits (the turn keeps it through a cancel).
    pub(crate) fn start_request(
        &self,
        method: &str,
        params: JsonValue,
    ) -> oneshot::Receiver<Result<JsonValue, RpcError>> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        if let Ok(mut map) = self.waiting.lock() {
            map.insert(id, tx);
        }
        let line = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if self.out.send(line.to_string()).is_err() {
            // The sender drops with the entry: the receiver reads "closed".
            if let Ok(mut map) = self.waiting.lock() {
                map.remove(&id);
            }
        }
        rx
    }

    /// Send a request and wait for its reply.
    #[cfg(test)]
    pub(crate) async fn request(
        &self,
        method: &str,
        params: JsonValue,
    ) -> Result<JsonValue, RpcError> {
        self.start_request(method, params)
            .await
            .unwrap_or_else(|_| Err(RpcError::closed()))
    }

    pub(crate) fn notify(&self, method: &str, params: JsonValue) {
        let line = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        let _ = self.out.send(line.to_string());
    }

    pub(crate) fn reply(&self, id: JsonValue, result: JsonValue) {
        let line = json!({ "jsonrpc": "2.0", "id": id, "result": result });
        let _ = self.out.send(line.to_string());
    }

    pub(crate) fn reply_error(&self, id: JsonValue, code: i64, message: &str) {
        let line = json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message }
        });
        let _ = self.out.send(line.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader};

    #[tokio::test]
    async fn requests_get_their_replies_and_the_agent_s_messages_are_routed() {
        let (ours, theirs) = duplex(64 * 1024);
        let (read, write) = tokio::io::split(ours);
        let (peer, mut incoming, _pumps) = connect(read, write);
        let (their_read, mut their_write) = tokio::io::split(theirs);
        let mut their_lines = BufReader::new(their_read).lines();

        let call = tokio::spawn({
            let peer = Arc::clone(&peer);
            async move { peer.request("initialize", json!({ "x": 1 })).await }
        });
        let sent: JsonValue =
            serde_json::from_str(&their_lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(sent["method"], "initialize");
        let id = sent["id"].clone();

        their_write
            .write_all(b"not json\n{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"a\":1}}\n")
            .await
            .unwrap();
        their_write
            .write_all(
                format!(
                    "{}\n",
                    json!({"jsonrpc":"2.0","id":7,"method":"fs/read_text_file","params":{}})
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        their_write
            .write_all(
                format!(
                    "{}\n",
                    json!({"jsonrpc":"2.0","id":id,"result":{"ok":true}})
                )
                .as_bytes(),
            )
            .await
            .unwrap();

        assert_eq!(call.await.unwrap().unwrap(), json!({ "ok": true }));
        assert_eq!(
            incoming.recv().await.unwrap(),
            Incoming::Notification {
                method: "session/update".into(),
                params: json!({ "a": 1 })
            }
        );
        assert!(matches!(
            incoming.recv().await.unwrap(),
            Incoming::Request { method, .. } if method == "fs/read_text_file"
        ));

        // The agent goes away: a waiting request fails, the queue ends.
        let waiting = tokio::spawn({
            let peer = Arc::clone(&peer);
            async move { peer.request("session/prompt", json!({})).await }
        });
        let _ = their_lines.next_line().await;
        drop(their_write);
        drop(their_lines);
        assert_eq!(waiting.await.unwrap().unwrap_err().code, RpcError::CLOSED);
        assert!(incoming.recv().await.is_none());
    }
}
