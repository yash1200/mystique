//! MCP stdio ↔ Mystique's HTTP MCP server.
//!
//! Some ACP agents can only start MCP servers as local processes (stdio is
//! the one transport every ACP agent must support). For them, `session/new`
//! names the Mystique app binary itself with [`BRIDGE_FLAG`]: the app then
//! starts no window, reads newline-delimited JSON-RPC from stdin, POSTs each
//! message to the local MCP server with the bearer token, and writes the
//! replies to stdout. It exits when stdin closes.
//!
//! The URL and token come from the environment ([`ENV_URL`], [`ENV_TOKEN`]),
//! set on the `mcpServers` stdio entry — never on the command line, never in
//! a log line. Only a loopback `http://` URL is accepted, so the token never
//! leaves the machine. Std only: no async runtime, no TLS.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use serde_json::{json, Value as JsonValue};

/// The app runs as the bridge when this is its first argument.
pub const BRIDGE_FLAG: &str = "--mcp-stdio-bridge";
/// Mystique's MCP endpoint, e.g. `http://127.0.0.1:7801/mcp`.
pub const ENV_URL: &str = "MYSTIQUE_MCP_URL";
/// The MCP bearer token.
pub const ENV_TOKEN: &str = "MYSTIQUE_MCP_TOKEN";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// A tool call may wait in the approval dock (90 s) and then run a query.
const READ_TIMEOUT: Duration = Duration::from_secs(600);
/// After stdin closes, how long replies still in flight may finish.
const DRAIN: Duration = Duration::from_secs(2);
/// A reply larger than this is refused (matches the ACP line limit).
const MAX_BODY: usize = 32 * 1024 * 1024;

/// Run as the bridge if the process was started with [`BRIDGE_FLAG`].
/// Returns the exit code; `None` means "start the app normally". Call it
/// first thing in `main`, before any window or runtime exists.
pub fn run_if_requested() -> Option<i32> {
    let first = std::env::args_os().nth(1)?;
    if first != BRIDGE_FLAG {
        return None;
    }
    let url = std::env::var(ENV_URL).unwrap_or_default();
    let token = std::env::var(ENV_TOKEN).unwrap_or_default();
    let stdin = std::io::stdin();
    let result = run(
        &url,
        &token,
        stdin.lock(),
        std::io::stdout(),
        std::io::stderr(),
    );
    Some(match result {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(std::io::stderr(), "mystique mcp bridge: {message}");
            2
        }
    })
}

/// Where the bridge posts: a loopback HTTP endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Endpoint {
    addr: SocketAddr,
    host: String,
    path: String,
}

impl Endpoint {
    /// Accepts `http://127.0.0.1:<port>/<path>`, `http://localhost:…` or
    /// `http://[::1]:…`. No query string (a token there would be logged by
    /// whoever logs URLs), no other host.
    pub(crate) fn parse(url: &str) -> Result<Self, String> {
        let rest = url
            .trim()
            .strip_prefix("http://")
            .ok_or("MYSTIQUE_MCP_URL must be a loopback http:// URL")?;
        if rest.contains('?') || rest.contains('#') || rest.contains('@') {
            return Err("MYSTIQUE_MCP_URL must not carry a query, fragment or user".into());
        }
        let (authority, path) = match rest.find('/') {
            Some(i) => (rest.get(..i).unwrap_or(""), rest.get(i..).unwrap_or("/")),
            None => (rest, "/"),
        };
        let (host, port) = authority
            .rsplit_once(':')
            .ok_or("MYSTIQUE_MCP_URL needs a port")?;
        let port: u16 = port
            .parse()
            .map_err(|_| "MYSTIQUE_MCP_URL has a bad port".to_string())?;
        let ip = match host {
            "127.0.0.1" | "localhost" => "127.0.0.1",
            "[::1]" => "::1",
            _ => return Err("MYSTIQUE_MCP_URL must point at this machine (127.0.0.1)".into()),
        };
        let addr = (ip, port)
            .to_socket_addrs()
            .map_err(|e| format!("bad MCP address: {e}"))?
            .next()
            .ok_or("bad MCP address")?;
        Ok(Self {
            addr,
            host: authority.to_string(),
            path: path.to_string(),
        })
    }
}

/// One HTTP reply, as far as the bridge cares.
struct Reply {
    status: u16,
    content_type: String,
    session_id: Option<String>,
    body: Vec<u8>,
}

fn post(
    endpoint: &Endpoint,
    token: &str,
    session_id: Option<&str>,
    body: &[u8],
) -> std::io::Result<Reply> {
    let mut stream = TcpStream::connect_timeout(&endpoint.addr, CONNECT_TIMEOUT)?;
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    stream.set_write_timeout(Some(CONNECT_TIMEOUT))?;
    let mut head = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        endpoint.path,
        endpoint.host,
        token,
        body.len()
    );
    if let Some(id) = session_id {
        head.push_str(&format!("Mcp-Session-Id: {id}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()?;
    read_reply(stream)
}

fn read_reply(stream: TcpStream) -> std::io::Result<Reply> {
    let bad = |what: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, what.to_string());
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let status: u16 = line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| bad("not an HTTP reply"))?;
    let mut content_type = String::new();
    let mut session_id = None;
    let mut length: Option<usize> = None;
    let mut chunked = false;
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-type" => content_type = value.to_ascii_lowercase(),
            "content-length" => length = value.parse().ok(),
            "transfer-encoding" => chunked = value.eq_ignore_ascii_case("chunked"),
            "mcp-session-id" => session_id = Some(value.to_string()),
            _ => {}
        }
    }
    let body = if chunked {
        read_chunked(&mut reader)?
    } else if let Some(n) = length {
        if n > MAX_BODY {
            return Err(bad("reply too large"));
        }
        let mut body = vec![0; n];
        reader.read_exact(&mut body)?;
        body
    } else {
        let mut body = Vec::new();
        reader
            .by_ref()
            .take(MAX_BODY as u64 + 1)
            .read_to_end(&mut body)?;
        if body.len() > MAX_BODY {
            return Err(bad("reply too large"));
        }
        body
    };
    Ok(Reply {
        status,
        content_type,
        session_id,
        body,
    })
}

fn read_chunked(reader: &mut impl BufRead) -> std::io::Result<Vec<u8>> {
    let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "bad chunked body");
    let mut body = Vec::new();
    let mut line = String::new();
    loop {
        line.clear();
        reader.read_line(&mut line)?;
        let size_text = line.trim().split(';').next().unwrap_or("");
        let size = usize::from_str_radix(size_text, 16).map_err(|_| bad())?;
        if size == 0 {
            return Ok(body);
        }
        if body.len() + size > MAX_BODY {
            return Err(bad());
        }
        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(body.get_mut(start..).ok_or_else(bad)?)?;
        line.clear();
        reader.read_line(&mut line)?;
    }
}

/// The JSON-RPC messages in an HTTP reply body: plain JSON, or the `data:`
/// lines of an event stream.
fn messages(reply: &Reply) -> Vec<JsonValue> {
    let text = String::from_utf8_lossy(&reply.body);
    if reply.content_type.starts_with("text/event-stream") {
        let mut out = Vec::new();
        for event in text.split("\n\n") {
            let data: Vec<&str> = event
                .lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .map(str::trim_start)
                .collect();
            if let Ok(value) = serde_json::from_str::<JsonValue>(&data.join("\n")) {
                out.push(value);
            }
        }
        return out;
    }
    match serde_json::from_str::<JsonValue>(text.trim()) {
        Ok(value) => vec![value],
        Err(_) => Vec::new(),
    }
}

fn rpc_error(id: &JsonValue, code: i64, message: &str) -> JsonValue {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

type Shared<W> = Arc<Mutex<W>>;

struct Bridge<W, L> {
    endpoint: Endpoint,
    token: String,
    session: Mutex<Option<String>>,
    out: Shared<W>,
    log: Shared<L>,
    inflight: Mutex<usize>,
    idle: Condvar,
}

impl<W: Write, L: Write> Bridge<W, L> {
    fn emit(&self, value: &JsonValue) {
        if let Ok(mut out) = self.out.lock() {
            // Compact JSON has no raw newlines: one message per line.
            let _ = writeln!(out, "{value}");
            let _ = out.flush();
        }
    }

    /// Diagnostics go to stderr. They never carry the token: nothing below
    /// formats it, and the masking is a second guard.
    fn note(&self, message: &str) {
        let message = if self.token.len() >= 6 {
            message.replace(&self.token, "[redacted]")
        } else {
            message.to_string()
        };
        if let Ok(mut log) = self.log.lock() {
            let _ = writeln!(log, "mystique mcp bridge: {message}");
        }
    }

    fn forward(&self, line: &str) {
        let parsed: JsonValue = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                self.emit(&rpc_error(
                    &JsonValue::Null,
                    -32700,
                    &format!("parse error: {e}"),
                ));
                return;
            }
        };
        // The id to answer on failure (none for a notification).
        let id = parsed
            .get("id")
            .cloned()
            .filter(|_| parsed.get("method").is_some());
        let session = self.session.lock().ok().and_then(|s| s.clone());
        let fail = |code: i64, message: &str| {
            if let Some(id) = &id {
                self.emit(&rpc_error(id, code, message));
            }
        };
        let reply = match post(
            &self.endpoint,
            &self.token,
            session.as_deref(),
            line.as_bytes(),
        ) {
            Ok(reply) => reply,
            Err(e) => {
                self.note(&format!("request failed: {e}"));
                fail(
                    -32603,
                    "Mystique's MCP server is not reachable. Is Mystique running with the MCP server on?",
                );
                return;
            }
        };
        if let Some(sid) = reply.session_id.clone() {
            if let Ok(mut s) = self.session.lock() {
                *s = Some(sid);
            }
        }
        match reply.status {
            200..=299 => {
                for message in messages(&reply) {
                    self.emit(&message);
                }
            }
            401 | 403 => {
                self.note(&format!(
                    "the MCP server refused the token (HTTP {})",
                    reply.status
                ));
                fail(-32001, "Mystique's MCP server refused the token.");
            }
            status => {
                self.note(&format!("the MCP server answered HTTP {status}"));
                // A JSON-RPC error body is still the best answer.
                let replies = messages(&reply);
                if replies.is_empty() {
                    fail(
                        -32603,
                        &format!("Mystique's MCP server answered HTTP {status}."),
                    );
                } else {
                    for message in replies {
                        self.emit(&message);
                    }
                }
            }
        }
    }

    fn done(&self) {
        if let Ok(mut n) = self.inflight.lock() {
            *n = n.saturating_sub(1);
            self.idle.notify_all();
        }
    }
}

/// Bridge `input` (MCP over stdio, one JSON-RPC message per line) to the
/// HTTP endpoint. Each message is posted on its own thread, so a tool call
/// waiting for approval does not hold up the rest. Returns when `input`
/// ends, after giving replies in flight [`DRAIN`] to land.
pub fn run<R, W, L>(url: &str, token: &str, input: R, output: W, log: L) -> Result<(), String>
where
    R: BufRead,
    W: Write + Send + 'static,
    L: Write + Send + 'static,
{
    let endpoint = Endpoint::parse(url)?;
    if token.trim().is_empty() {
        return Err(format!("{ENV_TOKEN} is not set"));
    }
    let bridge = Arc::new(Bridge {
        endpoint,
        token: token.trim().to_string(),
        session: Mutex::new(None),
        out: Arc::new(Mutex::new(output)),
        log: Arc::new(Mutex::new(log)),
        inflight: Mutex::new(0),
        idle: Condvar::new(),
    });
    for line in input.lines() {
        let Ok(line) = line else { break };
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        if let Ok(mut n) = bridge.inflight.lock() {
            *n += 1;
        }
        let worker = Arc::clone(&bridge);
        std::thread::spawn(move || {
            worker.forward(&line);
            worker.done();
        });
    }
    // stdin closed: the agent is gone or done. Let quick replies land.
    if let Ok(n) = bridge.inflight.lock() {
        let _ = bridge.idle.wait_timeout_while(n, DRAIN, |n| *n > 0);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// A writer the test can read back.
    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);

    impl Write for Buf {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Buf {
        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap()).to_string()
        }
        fn lines(&self) -> Vec<JsonValue> {
            self.text()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect()
        }
    }

    /// One captured HTTP request: head and body.
    struct Seen {
        head: String,
        body: String,
    }

    /// A tiny HTTP server: answers each request with `respond(body)`.
    fn server(
        respond: impl Fn(&str) -> (u16, &'static str, String) + Send + Sync + 'static,
    ) -> (String, mpsc::Receiver<Seen>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://127.0.0.1:{}/mcp",
            listener.local_addr().unwrap().port()
        );
        let (tx, rx) = mpsc::channel();
        let respond = Arc::new(respond);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let tx = tx.clone();
                let respond = Arc::clone(&respond);
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut head = String::new();
                    let mut length = 0usize;
                    loop {
                        let mut line = String::new();
                        reader.read_line(&mut line).unwrap();
                        if line.trim().is_empty() {
                            break;
                        }
                        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                            length = v.trim().parse().unwrap();
                        }
                        head.push_str(&line);
                    }
                    let mut body = vec![0; length];
                    reader.read_exact(&mut body).unwrap();
                    let body = String::from_utf8(body).unwrap();
                    let (status, ctype, reply) = respond(&body);
                    let mut stream = stream;
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status} X\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nMcp-Session-Id: s-1\r\n\r\n{reply}",
                        reply.len()
                    );
                    let _ = tx.send(Seen { head, body });
                });
            }
        });
        (url, rx)
    }

    fn echo_result(body: &str) -> (u16, &'static str, String) {
        let msg: JsonValue = serde_json::from_str(body).unwrap();
        match msg.get("id") {
            Some(id) => (
                200,
                "application/json",
                json!({ "jsonrpc": "2.0", "id": id, "result": { "echo": msg["method"] } })
                    .to_string(),
            ),
            None => (202, "application/json", String::new()),
        }
    }

    const TOKEN: &str = "tok-SECRET-123456789";

    #[test]
    fn lines_are_forwarded_with_the_bearer_token_and_replies_come_back_one_per_line() {
        let (url, seen) = server(echo_result);
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            "\n\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n",
        );
        let out = Buf::default();
        let log = Buf::default();
        run(&url, TOKEN, input.as_bytes(), out.clone(), log.clone()).unwrap();
        let replies = out.lines();
        // The notification gets no reply; the request gets exactly one.
        assert_eq!(replies.len(), 1, "{}", out.text());
        assert_eq!(replies[0]["id"], 1);
        assert_eq!(replies[0]["result"]["echo"], "initialize");
        let heads: Vec<String> = (0..2)
            .map(|_| {
                let s = seen.recv_timeout(Duration::from_secs(2)).unwrap();
                assert!(s.body.starts_with('{'));
                s.head
            })
            .collect();
        for head in &heads {
            assert!(head.starts_with("POST /mcp HTTP/1.1"), "{head}");
            assert!(
                head.contains(&format!("Authorization: Bearer {TOKEN}")),
                "{head}"
            );
        }
        assert!(log.text().is_empty(), "{}", log.text());
    }

    /// stdin fed line by line from the test.
    struct Pipe {
        rx: mpsc::Receiver<Vec<u8>>,
        pending: Vec<u8>,
    }

    impl Read for Pipe {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.pending.is_empty() {
                match self.rx.recv() {
                    Ok(bytes) => self.pending = bytes,
                    Err(_) => return Ok(0),
                }
            }
            let n = buf.len().min(self.pending.len());
            buf[..n].copy_from_slice(&self.pending[..n]);
            self.pending.drain(..n);
            Ok(n)
        }
    }

    #[test]
    fn the_session_id_is_sent_back_after_the_server_assigns_one() {
        let (url, seen) = server(echo_result);
        let (tx, rx) = mpsc::channel();
        let out = Buf::default();
        let bridge_out = out.clone();
        let handle = std::thread::spawn(move || {
            let input = BufReader::new(Pipe {
                rx,
                pending: Vec::new(),
            });
            run(&url, TOKEN, input, bridge_out, Buf::default())
        });
        let line = |id: u32| format!("{}\n", json!({ "jsonrpc": "2.0", "id": id, "method": "m" }));
        tx.send(line(1).into_bytes()).unwrap();
        let first = seen.recv().unwrap();
        assert!(!first.head.to_ascii_lowercase().contains("mcp-session-id"));
        while out.lines().is_empty() {
            std::thread::sleep(Duration::from_millis(5));
        }
        tx.send(line(2).into_bytes()).unwrap();
        let second = seen.recv().unwrap();
        assert!(
            second.head.contains("Mcp-Session-Id: s-1"),
            "{}",
            second.head
        );
        drop(tx);
        handle.join().unwrap().unwrap();
        assert_eq!(out.lines().len(), 2);
    }

    #[test]
    fn event_stream_replies_and_errors_are_framed_as_json_rpc() {
        let (url, _) = server(|body| {
            let msg: JsonValue = serde_json::from_str(body).unwrap();
            match msg["method"].as_str() {
                Some("sse") => (
                    200,
                    "text/event-stream",
                    format!(
                        "event: message\ndata: {}\n\n",
                        json!({ "jsonrpc": "2.0", "id": msg["id"], "result": { "via": "sse" } })
                    ),
                ),
                Some("deny") => (401, "text/plain", "unauthorized".into()),
                _ => (500, "text/plain", "boom".into()),
            }
        });
        let input = [
            json!({ "jsonrpc": "2.0", "id": 1, "method": "sse" }).to_string(),
            json!({ "jsonrpc": "2.0", "id": "two", "method": "deny" }).to_string(),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "crash" }).to_string(),
            "not json".to_string(),
        ]
        .join("\n");
        let out = Buf::default();
        let log = Buf::default();
        run(&url, TOKEN, input.as_bytes(), out.clone(), log.clone()).unwrap();
        let mut replies = out.lines();
        replies.sort_by_key(|r| r["id"].to_string());
        let by_id = |id: JsonValue| replies.iter().find(|r| r["id"] == id).cloned().unwrap();
        assert_eq!(by_id(json!(1))["result"]["via"], "sse");
        assert_eq!(by_id(json!("two"))["error"]["code"], -32001);
        assert_eq!(by_id(json!(3))["error"]["code"], -32603);
        assert_eq!(by_id(JsonValue::Null)["error"]["code"], -32700);
        let log = log.text();
        assert!(log.contains("refused the token"), "{log}");
        assert!(!log.contains(TOKEN), "{log}");
        assert!(!out.text().contains(TOKEN));
    }

    #[test]
    fn an_unreachable_server_fails_requests_without_logging_the_token() {
        // Bind and drop: nothing listens on the port now.
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let url = format!("http://127.0.0.1:{port}/mcp");
        let input = json!({ "jsonrpc": "2.0", "id": 9, "method": "tools/list" }).to_string();
        let out = Buf::default();
        let log = Buf::default();
        run(&url, TOKEN, input.as_bytes(), out.clone(), log.clone()).unwrap();
        let replies = out.lines();
        assert_eq!(replies[0]["id"], 9);
        assert!(replies[0]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not reachable"));
        assert!(log.text().contains("request failed"));
        assert!(!log.text().contains(TOKEN));
    }

    #[test]
    fn only_loopback_urls_without_a_query_are_accepted() {
        assert!(Endpoint::parse("http://127.0.0.1:7801/mcp").is_ok());
        assert!(Endpoint::parse("http://localhost:7801/mcp").is_ok());
        assert!(Endpoint::parse("http://[::1]:7801/mcp").is_ok());
        for bad in [
            "https://127.0.0.1:7801/mcp",
            "http://example.com:80/mcp",
            "http://10.0.0.2:7801/mcp",
            "http://127.0.0.1/mcp",
            "http://127.0.0.1:7801/mcp?token=abc",
            "http://user@127.0.0.1:7801/mcp",
            "",
        ] {
            assert!(Endpoint::parse(bad).is_err(), "{bad}");
        }
        assert!(run(
            "http://127.0.0.1:1/mcp",
            " ",
            "".as_bytes(),
            Buf::default(),
            Buf::default()
        )
        .unwrap_err()
        .contains(ENV_TOKEN));
    }

    #[test]
    fn it_returns_when_stdin_closes() {
        let (url, _) = server(echo_result);
        let started = std::time::Instant::now();
        run(&url, TOKEN, "".as_bytes(), Buf::default(), Buf::default()).unwrap();
        assert!(started.elapsed() < Duration::from_millis(500));
    }
}
