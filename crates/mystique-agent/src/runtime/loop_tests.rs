//! The streaming loop against a fake provider, fake tools and a fake host.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mystique_core::ToolApprovalRequest;
use rig::completion::message::{AssistantContent, ToolCall, ToolFunction, UserContent};
use rig::completion::{Message, ToolDefinition, Usage};
use serde_json::{json, Value as JsonValue};
use tokio_util::sync::CancellationToken;

use super::events::{AgentErrorKind, AgentEvent};
use super::host::{RunHost, ToolFailure, ToolRunner};
use super::model::{BoxFuture, Delta, DeltaSink, TurnError, TurnModel, TurnOutput, TurnRequest};
use super::prompt::ErrorAt;
use super::runs::{execute_run, RunRegistry, RunSpec};
use super::stream::tests::recording_sink;
use super::turns::LoopConfig;
use super::AgentTableRef;

// ---------------------------------------------------------------- fakes

enum Step {
    /// Stream `deltas`, then answer with them and `calls` (id, name, args).
    Reply {
        deltas: Vec<&'static str>,
        calls: Vec<(&'static str, &'static str, JsonValue)>,
    },
    /// Fail, optionally after streaming some text.
    Fail {
        streamed: Option<&'static str>,
        retryable: bool,
    },
    /// Hang until the request is dropped.
    Block,
}

fn reply(text: &'static str) -> Step {
    Step::Reply {
        deltas: vec![text],
        calls: vec![],
    }
}

fn call(id: &'static str, name: &'static str, args: JsonValue) -> Step {
    Step::Reply {
        deltas: vec![],
        calls: vec![(id, name, args)],
    }
}

/// Sets its flag when dropped: proof the provider request was dropped.
struct DropFlag(Arc<AtomicBool>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct FakeModel {
    steps: Mutex<VecDeque<Step>>,
    /// (preamble, history) of every call.
    seen: Mutex<Vec<(String, Vec<Message>)>>,
    calls: AtomicUsize,
    blocked: Arc<AtomicBool>,
    dropped: Arc<AtomicBool>,
}

impl FakeModel {
    fn new(steps: Vec<Step>) -> Arc<Self> {
        Arc::new(Self {
            steps: Mutex::new(steps.into()),
            ..Self::default()
        })
    }

    fn history(&self, n: usize) -> Vec<Message> {
        self.seen.lock().unwrap()[n].1.clone()
    }
}

impl TurnModel for Arc<FakeModel> {
    fn turn<'a>(
        &'a self,
        req: TurnRequest,
        on_delta: DeltaSink<'a>,
    ) -> BoxFuture<'a, Result<TurnOutput, TurnError>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.seen
                .lock()
                .unwrap()
                .push((req.preamble.clone(), req.history.clone()));
            let step = self.steps.lock().unwrap().pop_front();
            match step {
                Some(Step::Reply { deltas, calls }) => {
                    let mut content = Vec::new();
                    for d in &deltas {
                        on_delta(Delta::Text((*d).to_string()));
                    }
                    if !deltas.is_empty() {
                        content.push(AssistantContent::text(deltas.concat()));
                    }
                    for (id, name, args) in calls {
                        content.push(AssistantContent::ToolCall(ToolCall::from_wire(
                            id,
                            ToolFunction::new(name.to_string(), args),
                        )));
                    }
                    let mut usage = Usage::new();
                    usage.input_tokens = 10;
                    usage.output_tokens = 5;
                    Ok(TurnOutput { content, usage })
                }
                Some(Step::Fail {
                    streamed,
                    retryable,
                }) => {
                    if let Some(text) = streamed {
                        on_delta(Delta::Text(text.to_string()));
                    }
                    Err(TurnError {
                        message: "503 overloaded".into(),
                        retryable,
                    })
                }
                Some(Step::Block) | None => {
                    let _flag = DropFlag(Arc::clone(&self.dropped));
                    self.blocked.store(true, Ordering::SeqCst);
                    std::future::pending::<()>().await;
                    unreachable!()
                }
            }
        })
    }
}

#[derive(Default)]
struct FakeTools {
    started: AtomicUsize,
    completed: AtomicUsize,
    args: Mutex<Vec<(String, JsonValue)>>,
    /// How long each call takes.
    delay: Mutex<Duration>,
    /// Per tool name: a canned result or failure.
    fail_with: Mutex<Option<ToolFailure>>,
    big_result: AtomicBool,
}

impl ToolRunner for Arc<FakeTools> {
    fn definitions(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            name: "describe_object".into(),
            description: "fake".into(),
            parameters: json!({ "type": "object" }),
        }]
    }

    fn call<'a>(
        &'a self,
        name: &'a str,
        args: JsonValue,
    ) -> BoxFuture<'a, Result<JsonValue, ToolFailure>> {
        Box::pin(async move {
            self.started.fetch_add(1, Ordering::SeqCst);
            self.args.lock().unwrap().push((name.to_string(), args));
            let delay = *self.delay.lock().unwrap();
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            self.completed.fetch_add(1, Ordering::SeqCst);
            if let Some(fail) = self.fail_with.lock().unwrap().clone() {
                return Err(fail);
            }
            if self.big_result.load(Ordering::SeqCst) {
                return Ok(json!({ "rows": [ "z".repeat(50_000) ] }));
            }
            Ok(json!({ "columns": [{ "name": "id", "dataType": "int4" }] }))
        })
    }
}

#[derive(Default)]
struct FakeHost {
    tools: Mutex<Vec<(String, String)>>,
    replies: Mutex<Vec<String>>,
    approvals: Mutex<Vec<ToolApprovalRequest>>,
}

impl RunHost for FakeHost {
    fn save_tool(&self, name: &str, content: &str) -> Option<String> {
        let mut tools = self.tools.lock().unwrap();
        tools.push((name.to_string(), content.to_string()));
        Some(format!("tool-{}", tools.len()))
    }

    fn save_reply(&self, content: &str) -> Result<String, String> {
        let mut replies = self.replies.lock().unwrap();
        replies.push(content.to_string());
        Ok(format!("reply-{}", replies.len()))
    }

    fn pending_approvals(&self) -> Vec<ToolApprovalRequest> {
        self.approvals.lock().unwrap().clone()
    }
}

struct Harness {
    model: Arc<FakeModel>,
    tools: Arc<FakeTools>,
    host: Arc<FakeHost>,
    config: LoopConfig,
    limit: Duration,
    mentions: Vec<AgentTableRef>,
}

impl Harness {
    fn new(steps: Vec<Step>) -> Self {
        Self {
            model: FakeModel::new(steps),
            tools: Arc::new(FakeTools::default()),
            host: Arc::new(FakeHost::default()),
            config: LoopConfig {
                retry_backoff: Duration::from_millis(5),
                approval_poll: Duration::from_millis(10),
                ..LoopConfig::default()
            },
            limit: Duration::from_secs(10),
            mentions: Vec::new(),
        }
    }

    /// Run to the terminal event with `token`. Returns every event.
    async fn run_with(self, token: CancellationToken) -> (Vec<AgentEvent>, Self) {
        let (sink, seen) = recording_sink();
        let spec = RunSpec {
            preamble: "contract".into(),
            mentions: self.mentions.clone(),
            history: vec![Message::user("question")],
            config: LoopConfig {
                max_turns: self.config.max_turns,
                max_tokens: self.config.max_tokens,
                retry_backoff: self.config.retry_backoff,
                approval_poll: self.config.approval_poll,
                context_budget: self.config.context_budget,
            },
            limit: self.limit,
        };
        execute_run(
            Box::new(Arc::clone(&self.model)),
            Arc::new(Arc::clone(&self.tools)),
            Arc::clone(&self.host) as Arc<dyn RunHost>,
            spec,
            token,
            sink,
        )
        .await;
        let events = seen.lock().unwrap().clone();
        (events, self)
    }

    async fn run(self) -> (Vec<AgentEvent>, Self) {
        self.run_with(CancellationToken::new()).await
    }
}

fn text_of(events: &[AgentEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::TextDelta { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn tool_result_text(msg: &Message) -> Option<String> {
    match msg {
        Message::User { content } => content.iter().find_map(|c| match c {
            UserContent::ToolResult(r) => Some(serde_json::to_string(&r.content).unwrap()),
            _ => None,
        }),
        _ => None,
    }
}

// ---------------------------------------------------------------- tests

#[tokio::test]
async fn a_two_tool_turn_streams_through_to_the_final_reply() {
    let h = Harness::new(vec![
        call("c1", "describe_object", json!({ "name": "orders" })),
        call("c2", "describe_object", json!({ "name": "users" })),
        Step::Reply {
            deltas: vec!["Here ", "is the ", "draft:\n```sql\nselect 1\n```"],
            calls: vec![],
        },
    ]);
    let (events, h) = h.run().await;

    let starts = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::ToolCallStart { .. }))
        .count();
    let ends: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::ToolCallEnd {
                ok,
                summary,
                message_id,
                ..
            } => Some((*ok, summary.clone(), message_id.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(starts, 2);
    assert_eq!(ends.len(), 2);
    assert_eq!(ends[0], (true, "1 columns".into(), Some("tool-1".into())));
    assert!(text_of(&events).contains("select 1"));
    match events.last() {
        Some(AgentEvent::Done { message_id, tokens }) => {
            assert_eq!(message_id, "reply-1");
            assert_eq!(tokens.input, 30);
            assert_eq!(tokens.output, 15);
        }
        other => panic!("expected done, got {other:?}"),
    }
    // Tool cards persist; the reply is the final text, not the first stub.
    assert_eq!(h.host.tools.lock().unwrap().len(), 2);
    assert_eq!(
        h.host.replies.lock().unwrap().clone(),
        vec!["Here is the draft:\n```sql\nselect 1\n```".to_string()]
    );
    // The third call saw both tool pairs, each call followed by its result.
    let third = h.model.history(2);
    assert_eq!(third.len(), 5);
    assert!(matches!(third[1], Message::Assistant { .. }));
    assert!(tool_result_text(&third[2]).is_some());
    assert!(tool_result_text(&third[4]).is_some());
}

#[tokio::test]
async fn stop_drops_the_provider_request_within_a_second() {
    let h = Harness::new(vec![Step::Block]);
    let registry = Arc::new(RunRegistry::default());
    let (run, token) = registry.begin("chat-1").unwrap();
    assert!(registry.begin("chat-1").is_none(), "one run per chat");
    let blocked = Arc::clone(&h.model.blocked);
    let dropped = Arc::clone(&h.model.dropped);
    let running = tokio::spawn(h.run_with(token));
    while !blocked.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let pressed = Instant::now();
    assert!(registry.cancel("chat-1"));
    let (events, _) = running.await.unwrap();
    registry.finish("chat-1", run);
    assert!(
        pressed.elapsed() < Duration::from_secs(1),
        "{:?}",
        pressed.elapsed()
    );
    assert!(
        dropped.load(Ordering::SeqCst),
        "provider request still alive"
    );
    assert!(matches!(
        events.last(),
        Some(AgentEvent::Error {
            kind: AgentErrorKind::Cancelled,
            ..
        })
    ));
    assert!(!registry.is_live("chat-1"));
    assert!(!registry.cancel("chat-1"));
}

#[tokio::test]
async fn a_timeout_aborts_the_run_and_no_tool_runs_after_it() {
    let mut h = Harness::new(vec![
        call("c1", "describe_object", json!({ "name": "a" })),
        call("c2", "describe_object", json!({ "name": "b" })),
        reply("never"),
    ]);
    // The first tool call outlives the time limit.
    *h.tools.delay.lock().unwrap() = Duration::from_millis(400);
    h.limit = Duration::from_millis(150);
    let tools = Arc::clone(&h.tools);
    let model = Arc::clone(&h.model);
    let started = Instant::now();
    let (events, h) = h.run().await;
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(matches!(
        events.last(),
        Some(AgentEvent::Error {
            kind: AgentErrorKind::Timeout,
            ..
        })
    ));
    // Long after the limit: the slow call never finished and nothing else ran.
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(tools.started.load(Ordering::SeqCst), 1);
    assert_eq!(tools.completed.load(Ordering::SeqCst), 0);
    assert_eq!(model.calls.load(Ordering::SeqCst), 1);
    assert!(h.host.tools.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_provider_failure_retries_only_while_nothing_streamed() {
    let h = Harness::new(vec![
        Step::Fail {
            streamed: None,
            retryable: true,
        },
        reply("ok"),
    ]);
    let (events, h) = h.run().await;
    assert_eq!(h.model.calls.load(Ordering::SeqCst), 2);
    assert!(matches!(events.last(), Some(AgentEvent::Done { .. })));

    let h = Harness::new(vec![
        Step::Fail {
            streamed: Some("partial answ"),
            retryable: true,
        },
        reply("never"),
    ]);
    let (events, h) = h.run().await;
    assert_eq!(
        h.model.calls.load(Ordering::SeqCst),
        1,
        "retried after streaming"
    );
    assert!(matches!(
        events.last(),
        Some(AgentEvent::Error {
            kind: AgentErrorKind::Provider,
            ..
        })
    ));
    // What the user saw is kept.
    assert!(h.host.replies.lock().unwrap()[0].starts_with("partial answ"));
}

#[tokio::test]
async fn large_tool_results_are_cut_for_the_model_only() {
    let h = Harness::new(vec![
        call("c1", "describe_object", json!({ "name": "big" })),
        reply("done"),
    ]);
    h.tools.big_result.store(true, Ordering::SeqCst);
    let (events, h) = h.run().await;
    let seen = tool_result_text(&h.model.history(1)[2]).unwrap();
    assert!(seen.contains("characters omitted"), "{}", &seen[..200]);
    assert!(seen.len() < 13_000, "{}", seen.len());
    // The card keeps everything.
    assert!(h.host.tools.lock().unwrap()[0].1.len() > 50_000);
    assert!(events.iter().any(|e| matches!(
        e,
        AgentEvent::ContextCompacted {
            trimmed_results: 1,
            ..
        }
    )));
}

#[tokio::test]
async fn a_failed_query_feeds_the_repair_prompt_with_sql_and_position() {
    let h = Harness::new(vec![
        call(
            "c1",
            "describe_object",
            json!({ "query": "select * FORM users" }),
        ),
        reply("fixed"),
    ]);
    *h.tools.fail_with.lock().unwrap() = Some(ToolFailure {
        message: "syntax error at or near \"FORM\"".into(),
        at: Some(ErrorAt {
            line: 1,
            column: 10,
        }),
    });
    let (events, h) = h.run().await;
    let seen = tool_result_text(&h.model.history(1)[2]).unwrap();
    assert!(seen.contains("Repair:"), "{seen}");
    assert!(seen.contains("select * FORM users"), "{seen}");
    assert!(seen.contains("syntax error"), "{seen}");
    assert!(seen.contains("line 1, column 10"), "{seen}");
    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolCallEnd { ok: false, .. })));
}

#[tokio::test]
async fn the_turn_budget_ends_in_exhausted_then_done() {
    let mut h = Harness::new(vec![
        call("c1", "describe_object", json!({})),
        call("c2", "describe_object", json!({})),
        call("c3", "describe_object", json!({})),
    ]);
    h.config.max_turns = 2;
    let (events, h) = h.run().await;
    let n = events.len();
    assert!(matches!(events[n - 2], AgentEvent::Exhausted { turns: 2 }));
    assert!(matches!(events[n - 1], AgentEvent::Done { .. }));
    assert_eq!(h.model.calls.load(Ordering::SeqCst), 2);
    assert!(h.host.replies.lock().unwrap()[0].contains("Continue"));
}

/// Log lines written while the test runs (current-thread runtime, so the
/// run's tasks log on this thread too).
#[derive(Clone, Default)]
struct LogBuf(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for LogBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn the_streamed_trace_log_and_tool_cards_redact_planted_passwords() {
    let log = LogBuf::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let h = Harness::new(vec![
        call(
            "c1",
            "describe_object",
            json!({
                "query": "select * from dblink('postgres://admin:hunter2@db.internal/app', 'x')",
                "password": "hunter2",
            }),
        ),
        reply("ok"),
    ]);
    let (events, h) = h.run().await;
    let trace = serde_json::to_string(&events).unwrap();
    assert!(trace.contains("toolCallStart"));
    assert!(!trace.contains("hunter2"), "{trace}");
    let logged = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    assert!(logged.contains("agent tool call"), "{logged}");
    assert!(!logged.contains("hunter2"), "{logged}");
    for (_, card) in h.host.tools.lock().unwrap().iter() {
        assert!(!card.contains("hunter2"), "{card}");
    }
    // The tool itself still got the real arguments.
    assert!(h.tools.args.lock().unwrap()[0]
        .1
        .to_string()
        .contains("hunter2"));
}

#[tokio::test]
async fn a_call_waiting_on_the_dock_is_announced() {
    let h = Harness::new(vec![
        call("c1", "describe_object", json!({ "query": "delete from t" })),
        reply("ok"),
    ]);
    *h.tools.delay.lock().unwrap() = Duration::from_millis(120);
    h.host.approvals.lock().unwrap().push(ToolApprovalRequest {
        id: "ap-1".into(),
        caller: "agent".into(),
        connection_id: "1".into(),
        connection_name: "prod".into(),
        database: None,
        environment: "production".into(),
        sql: "delete from t".into(),
        needed: "full".into(),
        granted: "read".into(),
        requested_at: String::new(),
        expires_at: String::new(),
    });
    let (events, _) = h.run().await;
    let pending: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            AgentEvent::ToolApprovalPending {
                approval_id,
                needed,
                ..
            } => Some((approval_id.clone(), needed.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(pending, vec![("ap-1".to_string(), "full".to_string())]);
}

#[tokio::test]
async fn table_mentions_send_their_columns_with_the_turn() {
    let mut h = Harness::new(vec![reply("ok")]);
    h.mentions = vec![AgentTableRef {
        schema: Some("public".into()),
        name: "orders".into(),
    }];
    let (_, h) = h.run().await;
    let preamble = h.model.seen.lock().unwrap()[0].0.clone();
    assert!(preamble.contains("public.orders: id int4"), "{preamble}");
    let args = h.tools.args.lock().unwrap().clone();
    assert_eq!(args[0].0, "describe_object");
    assert_eq!(args[0].1, json!({ "name": "orders", "schema": "public" }));
    // A lookup is context, not a tool card.
    assert!(h.host.tools.lock().unwrap().is_empty());
}
