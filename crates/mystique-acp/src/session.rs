//! One external-agent turn, start to finish: start the agent, `initialize`,
//! `session/new` with Mystique's MCP server, `session/prompt`, stream the
//! updates, answer the agent's requests, and stop the process.
//!
//! Stop and the time limit send `session/cancel`, give the agent
//! [`CANCEL_GRACE`] to end the turn, then kill its process group.

use std::sync::Arc;
use std::time::Duration;

use mystique_agent::kit::{CancellationToken, ChatHost, EventStream};
use mystique_agent::{AgentErrorKind, AgentEvent, AgentEventSink, AgentTokens};
use serde_json::Value as JsonValue;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use crate::mapping::Turn;
use crate::permission;
use crate::process::{self, AgentProcess, Children, Launch};
use crate::protocol::{self, AgentHello, TurnEnd};
use crate::rpc::{self, Incoming, Peer, Pumps, RpcError};
use crate::scrub::Scrubber;

/// After `session/cancel`, how long the agent has to end the turn.
pub(crate) const CANCEL_GRACE: Duration = Duration::from_millis(500);
/// `initialize` and `session/new` (a first `npx` run downloads the agent).
pub(crate) const SETUP_LIMIT: Duration = Duration::from_secs(90);
/// The Test button's sign-in check (`session/new` without MCP servers).
pub(crate) const PROBE_SESSION_LIMIT: Duration = Duration::from_secs(30);
/// The whole turn. Longer than the built-in agent's: a write may wait 90 s
/// in the approval dock, and agents plan before they call tools.
pub(crate) const RUN_LIMIT: Duration = Duration::from_secs(600);
/// How often a running Mystique call is checked against the dock.
const APPROVAL_POLL: Duration = Duration::from_millis(200);
/// Time the agent gets to exit after a finished turn, before the kill.
const EXIT_GRACE: Duration = Duration::from_millis(300);

/// Where the agent's stdio comes from: a real process, or a fake in tests.
pub(crate) struct Transport {
    pub reader: Box<dyn AsyncRead + Unpin + Send>,
    pub writer: Box<dyn AsyncWrite + Unpin + Send>,
    pub process: Option<AgentProcess>,
}

pub(crate) trait Launcher: Send + Sync {
    fn launch(&self, launch: &Launch, scrub: &Scrubber) -> std::io::Result<Transport>;
}

/// Spawns the configured command.
pub(crate) struct ProcessLauncher {
    pub children: Children,
}

impl Launcher for ProcessLauncher {
    fn launch(&self, launch: &Launch, scrub: &Scrubber) -> std::io::Result<Transport> {
        let (process, stdout, stdin) =
            process::spawn(launch, scrub.clone(), self.children.clone())?;
        Ok(Transport {
            reader: Box::new(stdout),
            writer: Box::new(stdin),
            process: Some(process),
        })
    }
}

/// Mystique's MCP server, as the agent reaches it.
#[derive(Clone)]
pub struct McpAccess {
    pub url: String,
    pub token: String,
}

impl std::fmt::Debug for McpAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpAccess")
            .field("url", &self.url)
            .field("token", &"[redacted]")
            .finish()
    }
}

enum Step {
    Reply(Result<JsonValue, RpcError>),
    Cancelled,
    TimedOut,
}

/// An open connection to the agent.
struct Connection {
    peer: Arc<Peer>,
    incoming: mpsc::UnboundedReceiver<Incoming>,
    pumps: Option<Pumps>,
    process: Option<AgentProcess>,
    scrub: Scrubber,
}

impl Connection {
    fn open(transport: Transport, scrub: Scrubber) -> Self {
        let (peer, incoming, pumps) = rpc::connect(transport.reader, transport.writer);
        Self {
            peer,
            incoming,
            pumps: Some(pumps),
            process: transport.process,
            scrub,
        }
    }

    /// Answer one message from the agent.
    fn serve(&self, msg: Incoming, turn: Option<&mut Turn>, session_id: Option<&str>) {
        match msg {
            Incoming::Notification { method, params } => {
                if method == protocol::SESSION_UPDATE {
                    if let (Some(turn), Some(sid)) = (turn, session_id) {
                        turn.on_notification(sid, params);
                    }
                }
            }
            Incoming::Request { id, method, params } => {
                if method == protocol::REQUEST_PERMISSION {
                    let call_id = params
                        .pointer("/toolCall/toolCallId")
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default();
                    let known = turn.as_ref().and_then(|t| t.identity_of(call_id)).cloned();
                    let answer = permission::answer(&params, known.as_ref());
                    let tool = self.scrub.text(&answer.tool);
                    if answer.allowed {
                        tracing::info!(tool = %tool, "acp: allowed a Mystique tool call once");
                    } else {
                        tracing::info!(tool = %tool, "acp: rejected a tool call that is not Mystique's");
                    }
                    self.peer.reply(id, answer.result);
                } else if method.starts_with("fs/") || method.starts_with("terminal/") {
                    tracing::warn!(method, "acp: the agent asked for file or terminal access");
                    self.peer.reply_error(
                        id,
                        -32601,
                        "Mystique does not offer file or terminal access.",
                    );
                } else {
                    tracing::debug!(method, "acp: unsupported request from the agent");
                    self.peer
                        .reply_error(id, -32601, &format!("method not found: {method}"));
                }
            }
        }
    }

    /// Wait for `reply`, serving the agent meanwhile. Ends early on
    /// `cancel` (if given) or at `deadline`.
    async fn serve_until(
        &mut self,
        reply: &mut oneshot::Receiver<Result<JsonValue, RpcError>>,
        mut turn: Option<&mut Turn>,
        session_id: Option<&str>,
        cancel: Option<&CancellationToken>,
        deadline: Instant,
    ) -> Step {
        let mut tick = tokio::time::interval(APPROVAL_POLL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let never = CancellationToken::new();
        let cancel = cancel.unwrap_or(&never);
        loop {
            tokio::select! {
                biased;
                r = &mut *reply => {
                    // What the agent sent before the reply is queued already:
                    // serve it first, or the turn's last updates are lost.
                    while let Ok(msg) = self.incoming.try_recv() {
                        self.serve(msg, turn.as_deref_mut(), session_id);
                    }
                    return Step::Reply(r.unwrap_or_else(|_| Err(RpcError::closed())));
                }
                _ = cancel.cancelled() => return Step::Cancelled,
                _ = tokio::time::sleep_until(deadline) => return Step::TimedOut,
                next = self.incoming.recv() => match next {
                    Some(msg) => self.serve(msg, turn.as_deref_mut(), session_id),
                    // The agent's stdout closed. Replies it sent came first,
                    // so a request still waiting will never be answered.
                    None => {
                        return Step::Reply(
                            reply.try_recv().unwrap_or_else(|_| Err(RpcError::closed())),
                        );
                    }
                },
                _ = tick.tick() => {
                    if let Some(turn) = turn.as_deref_mut() {
                        turn.check_approvals();
                    }
                }
            }
        }
    }

    fn start(
        &self,
        method: &str,
        params: JsonValue,
    ) -> oneshot::Receiver<Result<JsonValue, RpcError>> {
        self.peer.start_request(method, params)
    }

    fn stderr_tail(&self) -> String {
        self.process
            .as_ref()
            .map(AgentProcess::stderr_tail)
            .unwrap_or_default()
    }

    /// Close stdin, give the agent `grace`, then kill the process group.
    async fn close(mut self, grace: Duration) {
        drop(self.pumps.take());
        drop(self.peer);
        if let Some(process) = self.process.take() {
            process.stop(grace).await;
        }
    }
}

fn rpc_failure(step: &str, err: &RpcError, stderr: &str) -> String {
    let mut message = if err.code == RpcError::CLOSED {
        format!("The agent exited during {step}.")
    } else if err.code == protocol::AUTH_REQUIRED {
        "The agent needs you to sign in first. Run it once in a terminal and log in, or add its API key under Environment in Settings → AI.".to_string()
    } else {
        format!("The agent failed {step}: {}", err.message)
    };
    if !stderr.trim().is_empty() {
        message.push_str("\n\n");
        message.push_str(stderr.trim());
    }
    message
}

fn spawn_failure(launch: &Launch, err: &std::io::Error) -> String {
    format!("Could not start `{}`: {err}", launch.program)
}

/// Start the agent and run `initialize`. Returns the open connection.
async fn hello(
    launcher: &dyn Launcher,
    launch: &Launch,
    scrub: &Scrubber,
    cancel: Option<&CancellationToken>,
) -> Result<(Connection, AgentHello), Ended> {
    let transport = launcher
        .launch(launch, scrub)
        .map_err(|e| Ended::Failed(spawn_failure(launch, &e)))?;
    let mut conn = Connection::open(transport, scrub.clone());
    let mut reply = conn.start(protocol::INITIALIZE, protocol::initialize_params());
    let deadline = Instant::now() + SETUP_LIMIT;
    let result = match conn
        .serve_until(&mut reply, None, None, cancel, deadline)
        .await
    {
        Step::Reply(Ok(result)) => protocol::parse_initialize(result),
        Step::Reply(Err(err)) => Err(rpc_failure("initialize", &err, &conn.stderr_tail())),
        Step::Cancelled => {
            conn.close(Duration::ZERO).await;
            return Err(Ended::Cancelled);
        }
        Step::TimedOut => Err(format!(
            "The agent did not answer initialize in {}s.",
            SETUP_LIMIT.as_secs()
        )),
    };
    match result {
        Ok(hello) => Ok((conn, hello)),
        Err(message) => {
            conn.close(Duration::ZERO).await;
            Err(Ended::Failed(message))
        }
    }
}

/// What the Test button learned.
#[derive(Debug, Clone)]
pub(crate) struct ProbeOutcome {
    pub hello: AgentHello,
    /// `Some(false)`: `session/new` asked for sign-in. `None`: not known
    /// (the session was not tried, or failed for another reason).
    pub signed_in: Option<bool>,
    /// Why the test session failed, other than sign-in.
    pub session_problem: Option<String>,
}

/// Test button: start the agent, `initialize`, then — when the protocol
/// matches — open a session with no MCP server to see whether it is signed
/// in, and stop it.
pub(crate) async fn probe(
    launcher: &dyn Launcher,
    launch: &Launch,
    scrub: &Scrubber,
) -> Result<ProbeOutcome, String> {
    let (mut conn, hello) = match hello(launcher, launch, scrub, None).await {
        Ok(open) => open,
        Err(Ended::Failed(message)) => return Err(scrub.text(&message)),
        Err(_) => return Err("The agent did not start.".into()),
    };
    let mut outcome = ProbeOutcome {
        hello,
        signed_in: None,
        session_problem: None,
    };
    if outcome.hello.protocol_version == protocol::PROTOCOL.as_u16() {
        let mut reply = conn.start(
            protocol::SESSION_NEW,
            protocol::probe_session_params(&launch.cwd),
        );
        let deadline = Instant::now() + PROBE_SESSION_LIMIT;
        match conn
            .serve_until(&mut reply, None, None, None, deadline)
            .await
        {
            Step::Reply(Ok(_)) => outcome.signed_in = Some(true),
            Step::Reply(Err(err)) if err.code == protocol::AUTH_REQUIRED => {
                outcome.signed_in = Some(false);
            }
            Step::Reply(Err(err)) => {
                outcome.session_problem =
                    Some(scrub.text(&rpc_failure("session/new", &err, &conn.stderr_tail())));
            }
            Step::Cancelled | Step::TimedOut => {}
        }
    }
    conn.close(Duration::ZERO).await;
    Ok(outcome)
}

/// How a turn ended.
#[derive(Debug)]
pub(crate) enum Ended {
    Turn(TurnEnd),
    Cancelled,
    TimedOut,
    Failed(String),
}

/// Everything one turn needs.
pub(crate) struct TurnSpec {
    pub launch: Launch,
    pub mcp: McpAccess,
    /// The app binary, to run as the stdio MCP bridge for agents without
    /// HTTP MCP. `None`: such agents are refused.
    pub bridge: Option<std::path::PathBuf>,
    /// Text blocks of the prompt: Mystique's context, then the user's words.
    pub prompt: Vec<String>,
    pub scrub: Scrubber,
    pub host: ChatHost,
    pub limit: Duration,
}

/// Run the turn to its terminal event. Returns once every event is sent and
/// the agent's process is gone.
pub(crate) async fn execute(
    launcher: Arc<dyn Launcher>,
    spec: TurnSpec,
    token: CancellationToken,
    sink: AgentEventSink,
    on_hello: Box<dyn FnOnce(&AgentHello) + Send>,
) {
    let stream = EventStream::start(sink);
    let events = stream.sender();
    let mut turn = Turn::new(spec.scrub.clone(), events.clone(), spec.host.clone());
    let deadline = Instant::now() + spec.limit;
    let ended = drive(
        launcher.as_ref(),
        &spec,
        &token,
        deadline,
        &mut turn,
        on_hello,
    )
    .await;
    turn.finish();

    let scrub = &spec.scrub;
    let host = &spec.host;
    let save = |text: &str, note: Option<&str>| -> Result<String, String> {
        let text = scrub.text(text.trim());
        let body = match note {
            Some(note) if text.is_empty() => note.to_string(),
            Some(note) => format!("{text}\n\n{note}"),
            None => text,
        };
        host.save_reply(&body)
    };
    let keep_partial = |note: &str| {
        if !turn.reply().trim().is_empty() {
            let _ = save(turn.reply(), Some(note));
        }
    };
    let fail = |kind: AgentErrorKind, message: String| {
        events.send(AgentEvent::Error {
            kind,
            message: scrub.text(&message),
        });
    };
    let done = |saved: Result<String, String>| match saved {
        Ok(message_id) => events.send(AgentEvent::Done {
            message_id,
            tokens: AgentTokens::default(),
        }),
        Err(err) => fail(AgentErrorKind::Internal, err),
    };
    match ended {
        Ended::Turn(TurnEnd::EndTurn) => {
            if turn.reply().trim().is_empty() {
                fail(
                    AgentErrorKind::Provider,
                    "The agent finished without a reply.".into(),
                );
            } else {
                done(save(turn.reply(), None));
            }
        }
        Ended::Turn(TurnEnd::MaxTokens) => {
            done(save(
                turn.reply(),
                Some("_The agent hit its output limit._"),
            ));
        }
        Ended::Turn(TurnEnd::MaxTurnRequests) => {
            events.send(AgentEvent::Exhausted { turns: 0 });
            done(save(
                turn.reply(),
                Some("_The agent stopped at its step limit without a final answer. Press Continue to keep going._"),
            ));
        }
        Ended::Turn(TurnEnd::Refusal) => {
            keep_partial("_The agent refused here._");
            fail(
                AgentErrorKind::Provider,
                "The agent refused this request.".into(),
            );
        }
        Ended::Turn(TurnEnd::Cancelled) | Ended::Cancelled => {
            keep_partial("_Stopped._");
            fail(AgentErrorKind::Cancelled, "Stopped.".into());
        }
        Ended::TimedOut => {
            keep_partial("_Timed out._");
            fail(
                AgentErrorKind::Timeout,
                format!(
                    "The agent did not finish in {}s and was stopped.",
                    spec.limit.as_secs()
                ),
            );
        }
        Ended::Failed(message) => {
            keep_partial("_The agent failed here._");
            fail(AgentErrorKind::Provider, message);
        }
    }
    drop(turn);
    drop(events);
    stream.finish().await;
}

async fn drive(
    launcher: &dyn Launcher,
    spec: &TurnSpec,
    token: &CancellationToken,
    deadline: Instant,
    turn: &mut Turn,
    on_hello: Box<dyn FnOnce(&AgentHello) + Send>,
) -> Ended {
    let (mut conn, hello) = match hello(launcher, &spec.launch, &spec.scrub, Some(token)).await {
        Ok(open) => open,
        Err(ended) => return ended,
    };
    on_hello(&hello);
    let route = match protocol::mcp_route(&hello, spec.bridge.as_deref()) {
        Ok(route) => route,
        Err(reason) => {
            conn.close(Duration::ZERO).await;
            return Ended::Failed(reason);
        }
    };
    tracing::info!(
        transport = route.label(),
        "acp: passing Mystique's MCP server"
    );

    let setup_deadline = deadline.min(Instant::now() + SETUP_LIMIT);
    let params =
        protocol::new_session_params(&spec.launch.cwd, &route, &spec.mcp.url, &spec.mcp.token);
    let mut reply = conn.start(protocol::SESSION_NEW, params);
    let session_id = match conn
        .serve_until(&mut reply, None, None, Some(token), setup_deadline)
        .await
    {
        Step::Reply(Ok(result)) => match protocol::parse_new_session(result) {
            Ok(id) => id,
            Err(message) => {
                conn.close(Duration::ZERO).await;
                return Ended::Failed(message);
            }
        },
        Step::Reply(Err(err)) => {
            let message = rpc_failure("session/new", &err, &conn.stderr_tail());
            conn.close(Duration::ZERO).await;
            return Ended::Failed(message);
        }
        Step::Cancelled => {
            conn.close(Duration::ZERO).await;
            return Ended::Cancelled;
        }
        Step::TimedOut => {
            conn.close(Duration::ZERO).await;
            return if Instant::now() >= deadline {
                Ended::TimedOut
            } else {
                Ended::Failed(format!(
                    "The agent did not open a session in {}s.",
                    SETUP_LIMIT.as_secs()
                ))
            };
        }
    };

    let mut reply = conn.start(
        protocol::SESSION_PROMPT,
        protocol::prompt_params(&session_id, &spec.prompt),
    );
    let step = conn
        .serve_until(
            &mut reply,
            Some(turn),
            Some(&session_id),
            Some(token),
            deadline,
        )
        .await;
    let ended = match step {
        Step::Reply(Ok(result)) => match protocol::parse_prompt_response(result) {
            Ok(end) => Ended::Turn(end),
            Err(message) => Ended::Failed(message),
        },
        Step::Reply(Err(err)) => Ended::Failed(rpc_failure("the turn", &err, &conn.stderr_tail())),
        stopped @ (Step::Cancelled | Step::TimedOut) => {
            // Ask the agent to stop, keep serving it briefly, then kill it.
            conn.peer.notify(
                protocol::SESSION_CANCEL,
                protocol::cancel_params(&session_id),
            );
            let grace = Instant::now() + CANCEL_GRACE;
            let _ = conn
                .serve_until(&mut reply, Some(turn), Some(&session_id), None, grace)
                .await;
            conn.close(Duration::ZERO).await;
            return match stopped {
                Step::Cancelled => Ended::Cancelled,
                _ => Ended::TimedOut,
            };
        }
    };
    conn.close(EXIT_GRACE).await;
    ended
}
