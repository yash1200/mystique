//! End to end: a fake ACP agent (in-memory pipes) against the real
//! Mystique MCP server, the agent runtime's run registry and chat storage.
//! Process-level checks (kill on Stop, stderr masking) use real processes.

use std::collections::VecDeque;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mystique_agent::{
    AgentErrorKind, AgentEvent, AgentEventSink, AgentRuntime, AgentTableRef, SendAgentMessage,
};
use mystique_core::{
    AppDatabase, AppServices, ConnectionManager, SaveConnectionInput, SecretStore, SshConfig,
    ToolCaller,
};
use mystique_mcp::McpService;
use serde_json::{json, Value as JsonValue};
use tokio::io::{
    AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf,
    WriteHalf,
};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;

use crate::process::Launch;
use crate::scrub::{Scrubber, MASK};
use crate::service::AcpService;
use crate::session::{Launcher, McpAccess, Transport};
use crate::settings::{AcpEnvInput, SaveAcpAgent};

// ---------------------------------------------------------------- logs

#[derive(Clone, Default)]
struct LogBuf(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for LogBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut log) = self.0.lock() {
            log.extend_from_slice(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl LogBuf {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).to_string()
    }
}

fn captured_log() -> &'static LogBuf {
    static LOG: std::sync::OnceLock<LogBuf> = std::sync::OnceLock::new();
    LOG.get_or_init(|| {
        let log = LogBuf::default();
        let writer = log.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let _ = tracing::subscriber::set_global_default(subscriber);
        tracing::callsite::rebuild_interest_cache();
        log
    })
}

// ---------------------------------------------------------------- fixture

struct Fx {
    app: Arc<AppServices>,
    conn_id: String,
    dir: tempfile::TempDir,
    mcp: McpService,
    access: McpAccess,
    runtime: AgentRuntime,
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn fixture(environment: &str) -> Fx {
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(AppServices::with_secrets(
        AppDatabase::open_in_memory().unwrap(),
        ConnectionManager::bootstrap(),
        SecretStore::memory(),
    ));
    app.settings().set("ai.enabled", "true").unwrap();
    let conn_id = app
        .profiles()
        .save(SaveConnectionInput {
            id: None,
            name: "shop".into(),
            engine: "sqlite".into(),
            color_id: "teal".into(),
            environment: environment.into(),
            read_only: false,
            notes: None,
            plugin_config: json!({ "path": dir.path().join("shop.sqlite").to_string_lossy() }),
            password: None,
            secret_ref: None,
            clear_secret: false,
            copy_secret_from: None,
            favorite: None,
            ssh_config: SshConfig::default(),
            ssh_password: None,
            clear_ssh_secret: false,
            tls_key_passphrase: None,
            query_timeout_secs: None,
            save_password: None,
        })
        .unwrap()
        .id;
    app.settings()
        .set("mcp.port", &free_port().to_string())
        .unwrap();
    let mcp = McpService::new(Arc::clone(&app));
    let status = mcp.start().await.unwrap();
    let access = McpAccess {
        url: status.url.unwrap(),
        token: status.token.unwrap(),
    };
    Fx {
        app,
        conn_id,
        dir,
        mcp,
        access,
        runtime: AgentRuntime::new(),
    }
}

impl Fx {
    fn level(&self, level: &str) {
        self.app
            .tool_access()
            .set_level(&self.conn_id, None, Some(level))
            .unwrap();
    }

    async fn sql(&self, query: &str) -> JsonValue {
        let name = if query
            .trim_start()
            .to_ascii_uppercase()
            .starts_with("SELECT")
        {
            "execute_readonly_query"
        } else {
            "execute_query"
        };
        self.app
            .agent_tools()
            .call_tool(
                name,
                &json!({ "connection_id": self.conn_id, "query": query }),
                ToolCaller::Mcp,
            )
            .await
            .unwrap()
    }

    /// One custom agent, the default; configuring again replaces it.
    fn configure(&self, program: &str, args: &[&str], env: Vec<(&str, &str)>) {
        let existing = crate::settings::view(&self.app)
            .agents
            .first()
            .map(|a| a.id.clone());
        crate::settings::save_agent(
            &self.app,
            SaveAcpAgent {
                id: existing,
                catalog_id: None,
                label: None,
                make_default: true,
                command: program.into(),
                args: args.iter().map(|a| a.to_string()).collect(),
                env: env
                    .into_iter()
                    .map(|(n, v)| AcpEnvInput {
                        name: n.into(),
                        value: Some(v.into()),
                    })
                    .collect(),
            },
        )
        .unwrap();
    }

    fn service(&self, launcher: Arc<dyn Launcher>) -> AcpService {
        // The fake ignores the command, but settings must name a real file.
        let exe = std::env::current_exe().unwrap();
        self.configure(&exe.to_string_lossy(), &[], vec![]);
        AcpService::with_launcher(
            Arc::clone(&self.app),
            self.dir.path().join("work"),
            launcher,
        )
    }

    fn request(&self, text: &str, chat_id: Option<String>) -> SendAgentMessage {
        SendAgentMessage {
            connection_id: self.conn_id.clone(),
            connection_name: "shop".into(),
            engine: "sqlite".into(),
            database: None,
            chat_id,
            provider: "acp".into(),
            model: String::new(),
            text: text.into(),
            editor_query: Some("select 1".into()),
            mentions: vec![],
            fix: None,
        }
    }

    async fn close(self) {
        self.mcp.stop().await.unwrap();
        let _ = self
            .app
            .connections()
            .close_session(&format!("mcp:{}", self.conn_id))
            .await;
    }
}

type Events = Arc<Mutex<Vec<AgentEvent>>>;

fn recording_sink() -> (AgentEventSink, Events) {
    let seen: Events = Arc::default();
    let sink_seen = Arc::clone(&seen);
    let sink: AgentEventSink = Arc::new(move |event| sink_seen.lock().unwrap().push(event));
    (sink, seen)
}

fn terminal(events: &Events) -> Option<AgentEvent> {
    events
        .lock()
        .unwrap()
        .iter()
        .find(|e| matches!(e, AgentEvent::Done { .. } | AgentEvent::Error { .. }))
        .cloned()
}

async fn wait_terminal(events: &Events, limit: Duration) -> AgentEvent {
    let until = Instant::now() + limit;
    while Instant::now() < until {
        if let Some(end) = terminal(events) {
            return end;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("no terminal event: {:?}", events.lock().unwrap());
}

async fn wait_for(events: &Events, what: impl Fn(&AgentEvent) -> bool) {
    for _ in 0..500 {
        if events.lock().unwrap().iter().any(&what) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("event never came: {:?}", events.lock().unwrap());
}

fn streamed_text(events: &Events) -> String {
    events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            AgentEvent::TextDelta { text } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------- fake agent

type Script = Arc<dyn Fn(FakeAgent) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Every message the fake agent received, across launches.
type Seen = Arc<Mutex<Vec<JsonValue>>>;

struct FakeLauncher {
    script: Script,
    seen: Seen,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

impl FakeLauncher {
    fn new<F, Fut>(script: F) -> Arc<Self>
    where
        F: Fn(FakeAgent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        Arc::new(Self {
            script: Arc::new(move |agent| Box::pin(script(agent))),
            seen: Arc::default(),
            tasks: Mutex::default(),
        })
    }

    fn received(&self, method: &str) -> Vec<JsonValue> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m["method"] == method)
            .cloned()
            .collect()
    }
}

impl Launcher for FakeLauncher {
    fn launch(&self, _launch: &Launch, _scrub: &Scrubber) -> std::io::Result<Transport> {
        let (client, agent) = tokio::io::duplex(1 << 20);
        let (reader, writer) = tokio::io::split(client);
        let (agent_read, agent_write) = tokio::io::split(agent);
        let fake = FakeAgent {
            lines: BufReader::new(agent_read).lines(),
            out: agent_write,
            seen: Arc::clone(&self.seen),
            backlog: VecDeque::new(),
            next: 100,
        };
        let task = tokio::spawn((self.script)(fake));
        self.tasks.lock().unwrap().push(task);
        Ok(Transport {
            reader: Box::new(reader),
            writer: Box::new(writer),
            process: None,
        })
    }
}

struct FakeAgent {
    lines: Lines<BufReader<ReadHalf<DuplexStream>>>,
    out: WriteHalf<DuplexStream>,
    seen: Seen,
    backlog: VecDeque<JsonValue>,
    next: u64,
}

struct Handshake {
    session: String,
    url: String,
    auth: String,
    prompt_id: JsonValue,
}

impl FakeAgent {
    async fn read(&mut self) -> Option<JsonValue> {
        let line = self.lines.next_line().await.ok()??;
        let msg: JsonValue = serde_json::from_str(&line).unwrap();
        self.seen.lock().unwrap().push(msg.clone());
        Some(msg)
    }

    async fn recv(&mut self) -> Option<JsonValue> {
        if let Some(msg) = self.backlog.pop_front() {
            return Some(msg);
        }
        self.read().await
    }

    async fn send(&mut self, msg: JsonValue) {
        let _ = self.out.write_all(format!("{msg}\n").as_bytes()).await;
        let _ = self.out.flush().await;
    }

    async fn expect(&mut self, method: &str) -> (JsonValue, JsonValue) {
        loop {
            let msg = self.recv().await.expect("client hung up");
            if msg["method"] == method {
                return (msg["id"].clone(), msg["params"].clone());
            }
        }
    }

    async fn reply(&mut self, id: JsonValue, result: JsonValue) {
        self.send(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
            .await;
    }

    async fn update(&mut self, session: &str, update: JsonValue) {
        self.send(json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": { "sessionId": session, "update": update }
        }))
        .await;
    }

    async fn say(&mut self, session: &str, text: &str) {
        self.update(
            session,
            json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": text } }),
        )
        .await;
    }

    /// A request to Mystique; returns its reply (result or error object).
    async fn ask(&mut self, method: &str, params: JsonValue) -> JsonValue {
        self.next += 1;
        let id = self.next;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await;
        loop {
            let msg = self.read().await.expect("client hung up");
            if msg["id"] == json!(id) && msg.get("method").is_none() {
                return msg;
            }
            self.backlog.push_back(msg);
        }
    }

    async fn handshake(&mut self) -> Handshake {
        self.handshake_with(json!({ "mcpCapabilities": { "http": true, "sse": false } }))
            .await
    }

    /// The handshake, advertising `capabilities` as `agentCapabilities`.
    async fn handshake_with(&mut self, capabilities: JsonValue) -> Handshake {
        let (id, _) = self.expect("initialize").await;
        self.reply(
            id,
            json!({
                "protocolVersion": 1,
                "agentCapabilities": capabilities,
                "agentInfo": { "name": "fake-agent", "title": "Fake Agent", "version": "0.0.1" },
                "authMethods": []
            }),
        )
        .await;
        let (id, params) = self.expect("session/new").await;
        let server = &params["mcpServers"][0];
        let url = server["url"].as_str().unwrap_or_default().to_string();
        let auth = server["headers"][0]["value"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        self.reply(id, json!({ "sessionId": "s1" })).await;
        let (prompt_id, _) = self.expect("session/prompt").await;
        Handshake {
            session: "s1".into(),
            url,
            auth,
            prompt_id,
        }
    }

    async fn permission(&mut self, session: &str, call: JsonValue) -> JsonValue {
        let reply = self
            .ask(
                "session/request_permission",
                json!({
                    "sessionId": session,
                    "toolCall": call,
                    "options": [
                        { "optionId": "allow-always", "name": "Always allow", "kind": "allow_always" },
                        { "optionId": "allow-once", "name": "Allow", "kind": "allow_once" },
                        { "optionId": "reject-once", "name": "Reject", "kind": "reject_once" }
                    ]
                }),
            )
            .await;
        reply["result"]["outcome"].clone()
    }

    /// Run one of Mystique's tools over MCP and report it as a tool call.
    async fn mystique_tool(
        &mut self,
        hs: &Handshake,
        call_id: &str,
        tool: &str,
        args: JsonValue,
    ) -> JsonValue {
        let qualified = format!("mcp__mystique__{tool}");
        self.update(
            &hs.session,
            json!({
                "sessionUpdate": "tool_call",
                "toolCallId": call_id,
                "title": format!("mystique - {tool} (MCP)"),
                "kind": "other",
                "status": "pending",
                "rawInput": args,
                "_meta": { "claudeCode": { "toolName": qualified } }
            }),
        )
        .await;
        let outcome = self
            .permission(
                &hs.session,
                json!({
                    "toolCallId": call_id,
                    "title": format!("mystique - {tool} (MCP)"),
                    "_meta": { "claudeCode": { "toolName": qualified } }
                }),
            )
            .await;
        assert_eq!(outcome["optionId"], "allow-once", "{outcome}");
        let result = mcp_call(&hs.url, &hs.auth, tool, args).await;
        let failed = result["isError"] == json!(true);
        let text = result["content"][0]["text"].clone();
        self.update(
            &hs.session,
            json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": call_id,
                "status": if failed { "failed" } else { "completed" },
                "content": [{ "type": "content", "content": { "type": "text", "text": text } }]
            }),
        )
        .await;
        result
    }
}

/// `tools/call` on Mystique's MCP server over plain HTTP/1.1.
async fn mcp_call(url: &str, auth: &str, tool: &str, args: JsonValue) -> JsonValue {
    let rest = url.strip_prefix("http://").unwrap();
    let (host, path) = rest.split_once('/').unwrap();
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": tool, "arguments": args }
    })
    .to_string();
    let mut stream = TcpStream::connect(host).await.unwrap();
    let request = format!(
        "POST /{path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: {auth}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.unwrap();
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let reply: JsonValue = serde_json::from_str(body).unwrap();
    reply["result"].clone()
}

// ---------------------------------------------------------------- tests

#[tokio::test]
async fn the_test_button_initializes_and_declines_files_and_terminal() {
    let fx = fixture("local").await;
    let fake = FakeLauncher::new(|mut agent: FakeAgent| async move {
        let (id, _) = agent.expect("initialize").await;
        agent
            .reply(
                id,
                json!({
                    "protocolVersion": 1,
                    "agentCapabilities": { "loadSession": true, "mcpCapabilities": { "http": true } },
                    "agentInfo": { "name": "fake-agent", "title": "Fake Agent", "version": "0.0.1" },
                    "authMethods": [{ "id": "login", "name": "Log in with Claude" }]
                }),
            )
            .await;
        // The sign-in check: a session with no MCP server.
        let (id, _) = agent.expect("session/new").await;
        agent.reply(id, json!({ "sessionId": "probe" })).await;
        while agent.recv().await.is_some() {}
    });
    let acp = fx.service(fake.clone());
    let probe = acp.probe("").await.unwrap();
    assert_eq!(probe.name, "fake-agent");
    assert_eq!(probe.version, "0.0.1");
    assert_eq!(probe.protocol_version, 1);
    assert!(probe.mcp_http && probe.load_session);
    assert_eq!(probe.auth_methods, vec!["Log in with Claude".to_string()]);
    assert_eq!(probe.problem, None);
    assert_eq!(probe.signed_in, Some(true));
    assert_eq!(probe.mcp_transport.as_deref(), Some("http"));
    assert_eq!(probe.agent_id, "custom-1");
    assert_eq!(acp.settings().agents[0].label, "Fake Agent");

    let init = &fake.received("initialize")[0]["params"];
    assert_eq!(init["protocolVersion"], json!(1));
    assert_eq!(
        init["clientCapabilities"]["fs"],
        json!({ "readTextFile": false, "writeTextFile": false })
    );
    assert_eq!(init["clientCapabilities"]["terminal"], json!(false));
    // The probe's session carries no MCP server (and so no token).
    let session = &fake.received("session/new")[0]["params"];
    assert_eq!(session["mcpServers"], json!([]));
    assert!(fake.received("session/prompt").is_empty());
    fx.close().await;
}

#[tokio::test]
async fn a_turn_streams_text_and_a_tool_call_through_mystique_mcp() {
    let fx = fixture("local").await;
    fx.level("full");
    fx.sql("CREATE TABLE orders (id INTEGER PRIMARY KEY, total REAL)")
        .await;
    fx.level("read");
    let conn_id = fx.conn_id.clone();
    let fs_reply: Arc<Mutex<Option<JsonValue>>> = Arc::default();
    let fs_seen = Arc::clone(&fs_reply);
    let fake = FakeLauncher::new(move |mut agent: FakeAgent| {
        let conn_id = conn_id.clone();
        let fs_seen = Arc::clone(&fs_seen);
        async move {
            let hs = agent.handshake().await;
            agent.say(&hs.session, "Checking the tables. ").await;
            agent
                .update(
                    &hs.session,
                    json!({ "sessionUpdate": "agent_thought_chunk", "content": { "type": "text", "text": "Need the objects." } }),
                )
                .await;
            agent
                .update(
                    &hs.session,
                    json!({ "sessionUpdate": "plan", "entries": [
                        { "content": "List objects", "priority": "high", "status": "in_progress" },
                        { "content": "Draft a query", "priority": "medium", "status": "pending" }
                    ] }),
                )
                .await;
            // Declined capability: the client must refuse.
            let fs = agent
                .ask(
                    "fs/read_text_file",
                    json!({ "sessionId": hs.session, "path": "/etc/passwd" }),
                )
                .await;
            *fs_seen.lock().unwrap() = Some(fs);
            let result = agent
                .mystique_tool(
                    &hs,
                    "t1",
                    "list_objects",
                    json!({ "connection_id": conn_id }),
                )
                .await;
            assert_eq!(result["isError"], json!(false), "{result}");
            agent
                .say(
                    &hs.session,
                    "There is one table.\n```sql\nselect * from orders\n```",
                )
                .await;
            agent
                .reply(hs.prompt_id, json!({ "stopReason": "end_turn" }))
                .await;
            while agent.recv().await.is_some() {}
        }
    });
    let acp = fx.service(fake.clone());
    let (sink, events) = recording_sink();
    let started = acp
        .send(
            &fx.runtime,
            fx.access.clone(),
            fx.request("What tables are there?", None),
            sink,
        )
        .unwrap();
    assert_eq!(started.chat.provider, "acp");
    let end = wait_terminal(&events, Duration::from_secs(10)).await;
    let AgentEvent::Done { message_id, .. } = end else {
        panic!("{end:?}");
    };
    assert!(!fx.runtime.is_running(&started.chat.id));

    // Session setup: only Mystique's MCP server, over HTTP, with the token.
    let new = &fake.received("session/new")[0]["params"];
    let servers = new["mcpServers"].as_array().unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["type"], "http");
    assert_eq!(servers[0]["name"], "mystique");
    assert_eq!(servers[0]["url"], json!(fx.access.url));
    assert_eq!(
        servers[0]["headers"][0]["value"],
        json!(format!("Bearer {}", fx.access.token))
    );
    let cwd = PathBuf::from(new["cwd"].as_str().unwrap());
    assert!(cwd.is_absolute() && cwd.ends_with("work"), "{cwd:?}");
    // This agent takes no system prompt: the contract is the prompt's first
    // block, then Mystique's context, then the user's words.
    assert!(new.get("systemPrompt").is_none(), "{new}");
    let prompt = &fake.received("session/prompt")[0]["params"]["prompt"];
    let contract = prompt[0]["text"].as_str().unwrap();
    assert!(contract.contains("Task contract:"), "{contract}");
    assert!(contract.contains("`mystique` MCP server"), "{contract}");
    let context = prompt[1]["text"].as_str().unwrap();
    assert!(context.contains(&fx.conn_id));
    assert!(!context.contains("Task contract:"));
    assert_eq!(prompt[2]["text"], "What tables are there?");

    let fs = fs_reply.lock().unwrap().clone().unwrap();
    assert_eq!(fs["error"]["code"], json!(-32601), "{fs}");

    assert_eq!(
        streamed_text(&events),
        "Checking the tables. There is one table.\n```sql\nselect * from orders\n```"
    );
    let all = events.lock().unwrap().clone();
    let reasoning: String = all
        .iter()
        .filter_map(|e| match e {
            AgentEvent::ReasoningDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(reasoning.contains("Need the objects."), "{reasoning}");
    assert!(reasoning.contains("- [~] List objects"), "{reasoning}");
    assert!(all.iter().any(|e| matches!(e,
        AgentEvent::ToolCallStart { id, name, .. } if id == "t1" && name == "list_objects")));
    let card_id = all
        .iter()
        .find_map(|e| match e {
            AgentEvent::ToolCallEnd {
                id,
                ok: true,
                summary,
                message_id,
            } if id == "t1" => {
                assert_eq!(summary, "1 objects");
                message_id.clone()
            }
            _ => None,
        })
        .expect("tool call end");

    let rows = fx
        .app
        .agent_chats()
        .list_messages(&started.chat.id)
        .unwrap();
    let roles: Vec<&str> = rows.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "tool", "assistant"]);
    assert_eq!(rows[1].id, card_id);
    assert_eq!(rows[1].tool_name.as_deref(), Some("list_objects"));
    assert!(rows[1].content.contains("orders"), "{}", rows[1].content);
    assert_eq!(rows[2].id, message_id);
    assert_eq!(rows[2].draft_body.as_deref(), Some("select * from orders"));
    let chat = fx.app.agent_chats().get(&started.chat.id).unwrap();
    // The chat remembers its agent by id; the agent's name is remembered
    // on the agent for the menu.
    assert_eq!(chat.provider, "acp");
    assert_eq!(chat.model, "custom-1");
    assert_eq!(crate::settings::view(&fx.app).agents[0].label, "Fake Agent");
    fx.close().await;
}

#[tokio::test]
async fn the_contract_is_the_system_prompt_and_mentions_carry_their_columns() {
    let fx = fixture("local").await;
    fx.level("full");
    fx.sql("CREATE TABLE orders (id INTEGER PRIMARY KEY, total REAL)")
        .await;
    fx.level("read");
    let fake = FakeLauncher::new(|mut agent: FakeAgent| async move {
        let hs = agent
            .handshake_with(json!({
                "mcpCapabilities": { "http": true, "sse": false },
                "sessionCapabilities": { "systemPrompt": {} }
            }))
            .await;
        agent.say(&hs.session, "Totals are in orders.total.").await;
        agent
            .reply(hs.prompt_id, json!({ "stopReason": "end_turn" }))
            .await;
        while agent.recv().await.is_some() {}
    });
    let acp = fx.service(fake.clone());
    let (sink, events) = recording_sink();
    let mut req = fx.request("Sum the totals", None);
    req.mentions = vec![AgentTableRef {
        schema: None,
        name: "orders".into(),
    }];
    acp.send(&fx.runtime, fx.access.clone(), req, sink).unwrap();
    let end = wait_terminal(&events, Duration::from_secs(10)).await;
    assert!(matches!(end, AgentEvent::Done { .. }), "{end:?}");

    // The shared contract and the ACP rules, appended to the agent's own.
    let new = &fake.received("session/new")[0]["params"];
    assert_eq!(new["systemPrompt"]["mode"], "append", "{new}");
    let system = new["systemPrompt"]["content"][0]["text"].as_str().unwrap();
    assert!(system.contains("Task contract:"), "{system}");
    assert!(system.contains("Repair"), "{system}");
    assert!(system.contains("`mystique` MCP server"), "{system}");

    // The prompt: Mystique's context with the mentioned table's columns
    // (looked up before the turn), then the user's words. No contract.
    let prompt = &fake.received("session/prompt")[0]["params"]["prompt"];
    assert_eq!(prompt.as_array().unwrap().len(), 2, "{prompt}");
    let context = prompt[0]["text"].as_str().unwrap();
    assert!(!context.contains("Task contract:"), "{context}");
    assert!(context.contains(&fx.conn_id), "{context}");
    assert!(
        context.contains("Tables the user referenced with @"),
        "{context}"
    );
    assert!(context.contains("- orders: id"), "{context}");
    assert!(context.contains("total"), "{context}");
    assert_eq!(prompt[1]["text"], "Sum the totals");
    fx.close().await;
}

#[tokio::test]
async fn permission_is_allowed_once_for_mystique_tools_and_rejected_for_the_rest() {
    let fx = fixture("local").await;
    let outcomes: Arc<Mutex<Vec<(String, JsonValue)>>> = Arc::default();
    let seen = Arc::clone(&outcomes);
    let fake = FakeLauncher::new(move |mut agent: FakeAgent| {
        let seen = Arc::clone(&seen);
        async move {
            let hs = agent.handshake().await;
            for (label, call) in [
                (
                    "mystique",
                    json!({ "toolCallId": "a", "title": "Sample rows",
                    "_meta": { "claudeCode": { "toolName": "mcp__mystique__sample_rows" } } }),
                ),
                (
                    "bash",
                    json!({ "toolCallId": "b", "title": "rm -rf ~", "kind": "execute",
                    "_meta": { "claudeCode": { "toolName": "Bash" } } }),
                ),
                (
                    "edit",
                    json!({ "toolCallId": "c", "title": "Write ~/.zshrc", "kind": "edit" }),
                ),
                (
                    "other-server",
                    json!({ "toolCallId": "d", "title": "mcp__github__execute_query" }),
                ),
                (
                    "bare-name",
                    json!({ "toolCallId": "e", "title": "execute_query" }),
                ),
            ] {
                let outcome = agent.permission(&hs.session, call).await;
                seen.lock().unwrap().push((label.to_string(), outcome));
            }
            agent.say(&hs.session, "ok").await;
            agent
                .reply(hs.prompt_id, json!({ "stopReason": "end_turn" }))
                .await;
            while agent.recv().await.is_some() {}
        }
    });
    let acp = fx.service(fake);
    let (sink, events) = recording_sink();
    acp.send(&fx.runtime, fx.access.clone(), fx.request("hi", None), sink)
        .unwrap();
    assert!(matches!(
        wait_terminal(&events, Duration::from_secs(10)).await,
        AgentEvent::Done { .. }
    ));
    let outcomes = outcomes.lock().unwrap().clone();
    assert_eq!(outcomes.len(), 5);
    for (label, outcome) in outcomes {
        let want = if label == "mystique" {
            "allow-once"
        } else {
            "reject-once"
        };
        assert_eq!(outcome["outcome"], "selected", "{label}");
        assert_eq!(outcome["optionId"], want, "{label}");
    }
    fx.close().await;
}

#[tokio::test]
async fn stop_mid_turn_cancels_the_session_and_ends_within_a_second() {
    let fx = fixture("local").await;
    let fake = FakeLauncher::new(|mut agent: FakeAgent| async move {
        let hs = agent.handshake().await;
        agent.say(&hs.session, "Working on it").await;
        // A stubborn agent: it reads session/cancel but never ends the turn.
        while agent.recv().await.is_some() {}
    });
    let acp = fx.service(fake.clone());
    let (sink, events) = recording_sink();
    let started = acp
        .send(
            &fx.runtime,
            fx.access.clone(),
            fx.request("long job", None),
            sink,
        )
        .unwrap();
    wait_for(&events, |e| matches!(e, AgentEvent::TextDelta { .. })).await;
    assert!(fx.runtime.is_running(&started.chat.id));

    let stopped_at = Instant::now();
    assert!(fx.runtime.cancel(&started.chat.id));
    let end = wait_terminal(&events, Duration::from_secs(3)).await;
    let took = stopped_at.elapsed();
    assert!(took < Duration::from_millis(1200), "Stop took {took:?}");
    assert!(
        matches!(
            &end,
            AgentEvent::Error {
                kind: AgentErrorKind::Cancelled,
                ..
            }
        ),
        "{end:?}"
    );
    assert_eq!(fake.received("session/cancel").len(), 1);
    assert_eq!(
        fake.received("session/cancel")[0]["params"]["sessionId"],
        "s1"
    );
    // The chat is free again and the partial reply is kept.
    for _ in 0..100 {
        if !fx.runtime.is_running(&started.chat.id) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!fx.runtime.is_running(&started.chat.id));
    let rows = fx
        .app
        .agent_chats()
        .list_messages(&started.chat.id)
        .unwrap();
    let last = rows.last().unwrap();
    assert_eq!(last.role, "assistant");
    assert!(
        last.content.starts_with("Working on it"),
        "{}",
        last.content
    );
    assert!(last.content.contains("_Stopped._"));
    fx.close().await;
}

#[tokio::test]
async fn a_write_waits_in_the_dock_and_is_denied_on_timeout() {
    let fx = fixture("production").await;
    fx.level("full");
    fx.sql("CREATE TABLE t (id INTEGER)").await;
    fx.level("read");
    fx.app.tool_access().set_ask(&fx.conn_id, true).unwrap();
    fx.app
        .tool_approvals()
        .set_timeout(Duration::from_millis(700));
    let conn_id = fx.conn_id.clone();
    let fake = FakeLauncher::new(move |mut agent: FakeAgent| {
        let conn_id = conn_id.clone();
        async move {
            let hs = agent.handshake().await;
            let result = agent
                .mystique_tool(
                    &hs,
                    "w1",
                    "execute_query",
                    json!({ "connection_id": conn_id, "query": "INSERT INTO t VALUES (1)" }),
                )
                .await;
            assert_eq!(result["isError"], json!(true), "{result}");
            agent.say(&hs.session, "The insert was not approved.").await;
            agent
                .reply(hs.prompt_id, json!({ "stopReason": "end_turn" }))
                .await;
            while agent.recv().await.is_some() {}
        }
    });
    let acp = fx.service(fake);
    let (sink, events) = recording_sink();
    let started = acp
        .send(
            &fx.runtime,
            fx.access.clone(),
            fx.request("add a row", None),
            sink,
        )
        .unwrap();

    // While it waits, the dock holds exactly that statement for MCP.
    let req = loop {
        if let Some(req) = fx.app.tool_approvals().pending().into_iter().next() {
            break req;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    assert_eq!(req.caller, "mcp");
    assert_eq!(req.sql, "INSERT INTO t VALUES (1)");
    assert_eq!(req.connection_id, fx.conn_id);

    assert!(matches!(
        wait_terminal(&events, Duration::from_secs(10)).await,
        AgentEvent::Done { .. }
    ));
    let all = events.lock().unwrap().clone();
    let pending = all.iter().position(|e| {
        matches!(e,
        AgentEvent::ToolApprovalPending { id, sql, needed, .. }
            if id == "w1" && sql == "INSERT INTO t VALUES (1)" && needed == "write")
    });
    let ended = all.iter().position(|e| {
        matches!(e,
        AgentEvent::ToolCallEnd { id, ok: false, summary, .. }
            if id == "w1" && summary.contains("nobody approved"))
    });
    assert!(pending.is_some() && ended.is_some(), "{all:?}");
    assert!(pending < ended);
    // Nothing ran.
    let count = fx.sql("SELECT count(*) FROM t").await;
    assert_eq!(count["rows"][0][0], json!(0), "{count}");
    let rows = fx
        .app
        .agent_chats()
        .list_messages(&started.chat.id)
        .unwrap();
    assert!(rows
        .iter()
        .any(|m| m.role == "tool" && m.tool_name.as_deref() == Some("execute_query")));
    fx.close().await;
}

#[tokio::test]
async fn planted_secrets_never_reach_the_stream_logs_or_saved_rows() {
    let log = captured_log();
    let fx = fixture("local").await;
    let token = fx.access.token.clone();
    let planted = "sk-planted-SECRET-4242424242";
    let (head, tail) = token.split_at(token.len() / 2);
    let (head, tail) = (head.to_string(), tail.to_string());
    let fake = FakeLauncher::new(move |mut agent: FakeAgent| {
        let (head, tail) = (head.clone(), tail.clone());
        async move {
            let hs = agent.handshake().await;
            // The token, split across chunks, and the env secret.
            agent.say(&hs.session, &format!("my token is {head}")).await;
            agent
                .say(
                    &hs.session,
                    &format!("{tail} and the key is sk-planted-SEC"),
                )
                .await;
            agent.say(&hs.session, "RET-4242424242. Bye.").await;
            agent
                .update(
                    &hs.session,
                    json!({
                        "sessionUpdate": "tool_call",
                        "toolCallId": "x1",
                        "title": format!("curl -H 'Authorization: Bearer {head}{tail}'"),
                        "kind": "fetch",
                        "rawInput": { "header": format!("Bearer {head}{tail}"), "key": "sk-planted-SECRET-4242424242" }
                    }),
                )
                .await;
            let outcome = agent
                .permission(
                    &hs.session,
                    json!({ "toolCallId": "x1", "title": format!("curl sk-planted-SECRET-4242424242 {head}{tail}") }),
                )
                .await;
            assert_eq!(outcome["optionId"], "reject-once");
            agent
                .update(
                    &hs.session,
                    json!({
                        "sessionUpdate": "tool_call_update",
                        "toolCallId": "x1",
                        "status": "failed",
                        "content": [{ "type": "content", "content": { "type": "text",
                            "text": "denied; key sk-planted-SECRET-4242424242" } }]
                    }),
                )
                .await;
            agent
                .reply(hs.prompt_id, json!({ "stopReason": "end_turn" }))
                .await;
            while agent.recv().await.is_some() {}
        }
    });
    let acp = fx.service(fake);
    fx.configure(
        &std::env::current_exe().unwrap().to_string_lossy(),
        &[],
        vec![("MY_API_KEY", planted)],
    );
    let (sink, events) = recording_sink();
    let started = acp
        .send(
            &fx.runtime,
            fx.access.clone(),
            fx.request("leak please", None),
            sink,
        )
        .unwrap();
    assert!(matches!(
        wait_terminal(&events, Duration::from_secs(10)).await,
        AgentEvent::Done { .. }
    ));
    let stream = serde_json::to_string(&*events.lock().unwrap()).unwrap();
    let saved = serde_json::to_string(
        &fx.app
            .agent_chats()
            .list_messages(&started.chat.id)
            .unwrap(),
    )
    .unwrap();
    for (what, text) in [
        ("stream", &stream),
        ("saved rows", &saved),
        ("log", &log.text()),
    ] {
        assert!(!text.contains(&token), "MCP token in the {what}: {text}");
        assert!(!text.contains(planted), "planted key in the {what}: {text}");
    }
    assert!(
        streamed_text(&events).contains(&format!("my token is {MASK} and the key is {MASK}. Bye."))
    );
    assert!(stream.contains("toolCallEnd"), "{stream}");
    fx.close().await;
}

#[tokio::test]
async fn an_agent_that_exits_or_lacks_http_mcp_fails_the_turn_with_a_reason() {
    let fx = fixture("local").await;
    // Exits while Mystique waits for session/new.
    let fake = FakeLauncher::new(|mut agent: FakeAgent| async move {
        let (id, _) = agent.expect("initialize").await;
        agent
            .reply(
                id,
                json!({ "protocolVersion": 1, "agentCapabilities": { "mcpCapabilities": { "http": true } } }),
            )
            .await;
        agent.expect("session/new").await;
    });
    let acp = fx.service(fake);
    let (sink, events) = recording_sink();
    let started = Instant::now();
    acp.send(&fx.runtime, fx.access.clone(), fx.request("hi", None), sink)
        .unwrap();
    let end = wait_terminal(&events, Duration::from_secs(5)).await;
    assert!(started.elapsed() < Duration::from_secs(2));
    let AgentEvent::Error { kind, message } = end else {
        panic!("{end:?}");
    };
    assert_eq!(kind, AgentErrorKind::Provider);
    assert!(message.contains("exited during session/new"), "{message}");

    // Cannot reach MCP over HTTP: refused before a session opens.
    let fake = FakeLauncher::new(|mut agent: FakeAgent| async move {
        let (id, _) = agent.expect("initialize").await;
        agent
            .reply(id, json!({ "protocolVersion": 1, "agentCapabilities": {} }))
            .await;
        while agent.recv().await.is_some() {}
    });
    let acp = fx.service(fake.clone());
    let (sink, events) = recording_sink();
    acp.send(&fx.runtime, fx.access.clone(), fx.request("hi", None), sink)
        .unwrap();
    let end = wait_terminal(&events, Duration::from_secs(5)).await;
    assert!(
        matches!(&end, AgentEvent::Error { message, .. } if message.contains("over HTTP")),
        "{end:?}"
    );
    assert!(fake.received("session/new").is_empty());

    // The MCP server is off: refused before anything starts.
    let err = acp
        .send(
            &fx.runtime,
            McpAccess {
                url: String::new(),
                token: String::new(),
            },
            fx.request("hi", None),
            recording_sink().0,
        )
        .err()
        .unwrap();
    assert!(err.to_string().contains("Settings → MCP"), "{err}");
    fx.close().await;
}

#[tokio::test]
async fn an_agent_without_http_mcp_reaches_mystique_through_the_stdio_bridge() {
    let fx = fixture("local").await;
    fx.level("full");
    fx.sql("CREATE TABLE orders (id INTEGER PRIMARY KEY)").await;
    let conn_id = fx.conn_id.clone();
    type Bridged = Option<(JsonValue, String, String)>;
    let bridged: Arc<Mutex<Bridged>> = Arc::default();
    let seen = Arc::clone(&bridged);
    let fake = FakeLauncher::new(move |mut agent: FakeAgent| {
        let conn_id = conn_id.clone();
        let seen = Arc::clone(&seen);
        async move {
            let (id, _) = agent.expect("initialize").await;
            // stdio only: no mcpCapabilities at all.
            agent
                .reply(
                    id,
                    json!({ "protocolVersion": 1, "agentCapabilities": {}, "agentInfo": { "name": "stdio-agent", "version": "1" } }),
                )
                .await;
            let (id, params) = agent.expect("session/new").await;
            let server = params["mcpServers"][0].clone();
            let env = |name: &str| {
                server["env"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|e| e["name"] == name)
                    .map(|e| e["value"].as_str().unwrap().to_string())
                    .unwrap()
            };
            let url = env(crate::bridge::ENV_URL);
            let token = env(crate::bridge::ENV_TOKEN);
            agent.reply(id, json!({ "sessionId": "s1" })).await;
            let (prompt_id, _) = agent.expect("session/prompt").await;
            // What the agent does: start the bridge and speak MCP to it.
            let input = [
                json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "fake", "version": "1" } } }),
                json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
                json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "list_objects", "arguments": { "connection_id": conn_id } } }),
            ]
            .iter()
            .map(|m| m.to_string())
            .collect::<Vec<_>>()
            .join("\n");
            let out = LogBuf::default();
            let log = LogBuf::default();
            let (o, l) = (out.clone(), log.clone());
            tokio::task::spawn_blocking(move || {
                crate::bridge::run(&url, &token, input.as_bytes(), o, l).unwrap();
            })
            .await
            .unwrap();
            *seen.lock().unwrap() = Some((server, out.text(), log.text()));
            agent.say("s1", "Done.").await;
            agent
                .reply(prompt_id, json!({ "stopReason": "end_turn" }))
                .await;
            while agent.recv().await.is_some() {}
        }
    });
    let exe = PathBuf::from("/Applications/Mystique.app/Contents/MacOS/mystique");
    let acp = fx.service(fake.clone()).with_bridge(exe.clone());
    let (sink, events) = recording_sink();
    let started = acp
        .send(
            &fx.runtime,
            fx.access.clone(),
            fx.request("tables?", None),
            sink,
        )
        .unwrap();
    let end = wait_terminal(&events, Duration::from_secs(10)).await;
    assert!(matches!(end, AgentEvent::Done { .. }), "{end:?}");
    // The chat remembers the (default) agent it ran with.
    let chat = fx.app.agent_chats().get(&started.chat.id).unwrap();
    assert_eq!(chat.model, "custom-1");

    let (server, out, log) = bridged.lock().unwrap().clone().unwrap();
    assert_eq!(server["command"], exe.to_string_lossy().as_ref());
    assert_eq!(server["args"], json!([crate::bridge::BRIDGE_FLAG]));
    assert!(!server["args"].to_string().contains(&fx.access.token));
    let replies: Vec<JsonValue> = out
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(replies.len(), 2, "{out}");
    let call = replies.iter().find(|r| r["id"] == 2).unwrap();
    assert_eq!(call["result"]["isError"], json!(false), "{call}");
    assert!(call.to_string().contains("orders"), "{call}");
    assert!(replies
        .iter()
        .any(|r| r["id"] == 1 && r["result"].is_object()));
    assert!(!log.contains(&fx.access.token), "{log}");
    fx.close().await;
}

#[tokio::test]
async fn the_test_button_reports_sign_in_and_how_mcp_is_reached() {
    let fx = fixture("local").await;
    let fake = FakeLauncher::new(|mut agent: FakeAgent| async move {
        let (id, _) = agent.expect("initialize").await;
        agent
            .reply(
                id,
                json!({ "protocolVersion": 1, "agentCapabilities": { "mcpCapabilities": { "http": true } } }),
            )
            .await;
        let (id, _) = agent.expect("session/new").await;
        agent
            .send(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": "Authentication required" } }))
            .await;
        while agent.recv().await.is_some() {}
    });
    let probe = fx.service(fake).probe("").await.unwrap();
    assert_eq!(probe.signed_in, Some(false));
    let problem = probe.problem.unwrap();
    assert!(problem.starts_with("Not signed in."), "{problem}");

    // A stdio-only agent: refused without the bridge, fine with it.
    let fake = FakeLauncher::new(|mut agent: FakeAgent| async move {
        let (id, _) = agent.expect("initialize").await;
        agent
            .reply(id, json!({ "protocolVersion": 1, "agentCapabilities": {} }))
            .await;
        let (id, _) = agent.expect("session/new").await;
        agent.reply(id, json!({ "sessionId": "p" })).await;
        while agent.recv().await.is_some() {}
    });
    let probe = fx.service(fake.clone()).probe("").await.unwrap();
    assert_eq!(probe.mcp_transport, None);
    assert!(probe.problem.unwrap().contains("bridge"));
    let probe = fx
        .service(fake)
        .with_bridge(PathBuf::from("/app/mystique"))
        .probe("")
        .await
        .unwrap();
    assert_eq!(probe.mcp_transport.as_deref(), Some("stdio"));
    assert_eq!(probe.signed_in, Some(true));
    assert_eq!(probe.problem, None);
    fx.close().await;
}

// ---------------------------------------------------------------- real processes

#[cfg(unix)]
async fn wait_gone(pid: u32) -> bool {
    for _ in 0..100 {
        // SAFETY: signal 0 only checks that the process exists.
        let alive = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
        if !alive {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

#[cfg(unix)]
#[tokio::test]
async fn stop_kills_the_agent_process_group() {
    let fx = fixture("local").await;
    let pid_file = fx.dir.path().join("pid");
    // The "agent" starts a helper (its own group member) and never speaks.
    let script = format!(
        "sleep 30 & echo $! > '{}'; wait",
        pid_file.to_string_lossy()
    );
    fx.configure("/bin/sh", &["-c", &script], vec![]);
    let acp = AcpService::new(Arc::clone(&fx.app), fx.dir.path());
    let (sink, events) = recording_sink();
    let started = acp
        .send(
            &fx.runtime,
            fx.access.clone(),
            fx.request("hello", None),
            sink,
        )
        .unwrap();
    let helper = loop {
        if let Ok(raw) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = raw.trim().parse::<u32>() {
                break pid;
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    let stopped_at = Instant::now();
    assert!(fx.runtime.cancel(&started.chat.id));
    let end = wait_terminal(&events, Duration::from_secs(3)).await;
    assert!(
        stopped_at.elapsed() < Duration::from_millis(1200),
        "{:?}",
        stopped_at.elapsed()
    );
    assert!(
        matches!(
            end,
            AgentEvent::Error {
                kind: AgentErrorKind::Cancelled,
                ..
            }
        ),
        "{end:?}"
    );
    assert!(wait_gone(helper).await, "the helper outlived Stop");
    fx.close().await;
}

#[cfg(unix)]
#[tokio::test]
async fn app_quit_kills_running_agents() {
    let fx = fixture("local").await;
    fx.configure("/bin/sh", &["-c", "sleep 30"], vec![]);
    let acp = AcpService::new(Arc::clone(&fx.app), fx.dir.path());
    let (sink, events) = recording_sink();
    acp.send(
        &fx.runtime,
        fx.access.clone(),
        fx.request("hello", None),
        sink,
    )
    .unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    while acp.running_agents() == 0 && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(acp.kill_all(), 1);
    let end = wait_terminal(&events, Duration::from_secs(3)).await;
    assert!(
        matches!(
            end,
            AgentEvent::Error {
                kind: AgentErrorKind::Provider,
                ..
            }
        ),
        "{end:?}"
    );
    fx.close().await;
}

#[cfg(unix)]
#[tokio::test]
async fn the_agent_s_stderr_is_logged_masked() {
    let log = captured_log();
    let fx = fixture("local").await;
    let planted = "sk-stderr-SECRET-777777777";
    fx.configure(
        "/bin/sh",
        &[
            "-c",
            "echo \"starting with $MY_API_KEY and $MYSTIQUE_MARK\" >&2; sleep 30",
        ],
        vec![
            ("MY_API_KEY", planted),
            ("MYSTIQUE_MARK", "stderr-probe-mark"),
        ],
    );
    let acp = AcpService::new(Arc::clone(&fx.app), fx.dir.path());
    let (sink, events) = recording_sink();
    let started = acp
        .send(
            &fx.runtime,
            fx.access.clone(),
            fx.request("hello", None),
            sink,
        )
        .unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !log.text().contains("stderr-probe-mark") && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    fx.runtime.cancel(&started.chat.id);
    wait_terminal(&events, Duration::from_secs(3)).await;
    let text = log.text();
    let line = text
        .lines()
        .find(|l| l.contains("stderr-probe-mark"))
        .expect("stderr was not logged");
    assert!(line.contains("mystique_acp::agent_stderr"), "{line}");
    assert!(line.contains(MASK), "{line}");
    assert!(!text.contains(planted));
    fx.close().await;
}
