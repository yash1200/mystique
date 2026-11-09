//! Live runs: one per chat, each cancellable.
//!
//! Stop and the time limit end a run the same way: cancel its
//! [`CancellationToken`] and `abort()` its task, then wait for the task to
//! be gone. Dropping the task drops the provider's HTTP request and any tool
//! call in flight, so nothing runs after the terminal event.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rig::completion::Message;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use super::events::{AgentErrorKind, AgentEvent};
use super::host::{RunHost, ToolRunner};
use super::model::TurnModel;
use super::prompt::referenced_tables;
use super::stream::{spawn_forwarder, AgentEventSink, EventTx, DELTA_FLUSH};
use super::turns::{run_turns, LoopConfig, LoopInput, LoopOutcome, Partial};
use super::AgentTableRef;

/// The whole run, every model call and tool call included.
pub(crate) const RUN_TIME_LIMIT: Duration = Duration::from_secs(180);
/// `@table` references looked up per turn.
const MAX_MENTIONS: usize = 8;

struct LiveRun {
    run: u64,
    token: CancellationToken,
}

/// Runs in flight, by chat id.
#[derive(Default)]
pub(crate) struct RunRegistry {
    live: Mutex<HashMap<String, LiveRun>>,
    next: AtomicU64,
}

impl RunRegistry {
    /// Claim the chat. `None` while another run of it is live.
    pub(crate) fn begin(&self, chat_id: &str) -> Option<(u64, CancellationToken)> {
        let mut live = self.live.lock().ok()?;
        if live.contains_key(chat_id) {
            return None;
        }
        let run = self.next.fetch_add(1, Ordering::SeqCst);
        let token = CancellationToken::new();
        live.insert(
            chat_id.to_string(),
            LiveRun {
                run,
                token: token.clone(),
            },
        );
        Some((run, token))
    }

    /// Stop the chat's run. False when none is live.
    pub(crate) fn cancel(&self, chat_id: &str) -> bool {
        let token = self
            .live
            .lock()
            .ok()
            .and_then(|live| live.get(chat_id).map(|r| r.token.clone()));
        match token {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }

    pub(crate) fn finish(&self, chat_id: &str, run: u64) {
        if let Ok(mut live) = self.live.lock() {
            if live.get(chat_id).is_some_and(|r| r.run == run) {
                live.remove(chat_id);
            }
        }
    }

    pub(crate) fn is_live(&self, chat_id: &str) -> bool {
        self.live
            .lock()
            .map(|live| live.contains_key(chat_id))
            .unwrap_or(false)
    }

    /// Stop every live run (app quit). Returns how many were live.
    pub(crate) fn cancel_all(&self) -> usize {
        let tokens: Vec<CancellationToken> = self
            .live
            .lock()
            .map(|live| live.values().map(|r| r.token.clone()).collect())
            .unwrap_or_default();
        for token in &tokens {
            token.cancel();
        }
        tokens.len()
    }
}

/// A claimed chat: its run is live until this is dropped. Stop and the app
/// quit cancel [`RunClaim::token`]; the run must end when it fires.
pub struct RunClaim {
    registry: Arc<RunRegistry>,
    chat_id: String,
    run: u64,
    token: CancellationToken,
}

impl RunClaim {
    /// Claim the chat. `None` while another run of it is live.
    pub(crate) fn begin(registry: &Arc<RunRegistry>, chat_id: &str) -> Option<Self> {
        let (run, token) = registry.begin(chat_id)?;
        Some(Self {
            registry: Arc::clone(registry),
            chat_id: chat_id.to_string(),
            run,
            token,
        })
    }

    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    pub fn chat_id(&self) -> &str {
        &self.chat_id
    }
}

impl Drop for RunClaim {
    fn drop(&mut self) {
        self.registry.finish(&self.chat_id, self.run);
    }
}

pub(crate) enum Supervised<T> {
    Finished(T),
    Cancelled,
    TimedOut,
    Panicked(String),
}

/// Run `work` on its own task until it ends, `token` is cancelled, or
/// `limit` passes. Cancel and timeout both abort the task and wait for it
/// to be dropped before returning.
pub(crate) async fn supervise<T, F>(
    work: F,
    token: &CancellationToken,
    limit: Duration,
) -> Supervised<T>
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    let mut handle = tokio::spawn(work);
    let ended = tokio::select! {
        joined = &mut handle => {
            return match joined {
                Ok(value) => Supervised::Finished(value),
                Err(err) => Supervised::Panicked(err.to_string()),
            };
        }
        _ = token.cancelled() => Supervised::Cancelled,
        _ = tokio::time::sleep(limit) => {
            token.cancel();
            Supervised::TimedOut
        }
    };
    handle.abort();
    let _ = handle.await;
    ended
}

/// Everything one run needs besides the model, tools and host.
pub(crate) struct RunSpec {
    /// Task contract + workspace context. Referenced tables are added here.
    pub preamble: String,
    pub mentions: Vec<AgentTableRef>,
    /// Earlier turns, then this run's prompt last.
    pub history: Vec<Message>,
    pub config: LoopConfig,
    pub limit: Duration,
}

/// The `@table` references with their columns (at most [`MAX_MENTIONS`]),
/// looked up with `describe_object`. Empty when there are none.
pub(crate) async fn describe_mentions(
    tools: &dyn ToolRunner,
    mentions: &[AgentTableRef],
) -> String {
    let mut described = Vec::new();
    for m in mentions.iter().take(MAX_MENTIONS) {
        let name = m.name.trim();
        if name.is_empty() {
            continue;
        }
        let schema = m.schema.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let label = schema.map_or_else(|| name.to_string(), |s| format!("{s}.{name}"));
        let args = match schema {
            Some(schema) => json!({ "name": name, "schema": schema }),
            None => json!({ "name": name }),
        };
        let result = tools
            .call("describe_object", args)
            .await
            .map_err(|f| f.message);
        described.push((label, result));
    }
    referenced_tables(&described)
}

fn with_note(text: &str, note: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        note.to_string()
    } else {
        format!("{text}\n\n{note}")
    }
}

/// Save what the user already saw before the run ended early.
fn keep_partial(host: &dyn RunHost, partial: &Partial, note: &str) {
    let text = partial.text();
    if !text.trim().is_empty() {
        let _ = host.save_reply(&with_note(&text, note));
    }
}

fn fail(events: &EventTx, kind: AgentErrorKind, message: impl Into<String>) {
    events.send(AgentEvent::Error {
        kind,
        message: message.into(),
    });
}

/// Run one turn to its terminal event. Returns when every event is sent.
pub(crate) async fn execute_run(
    model: Box<dyn TurnModel>,
    tools: Arc<dyn ToolRunner>,
    host: Arc<dyn RunHost>,
    spec: RunSpec,
    token: CancellationToken,
    sink: AgentEventSink,
) {
    let (events, forwarder) = spawn_forwarder(sink, DELTA_FLUSH);
    let partial = Partial::default();
    let limit = spec.limit;
    let work = {
        let events = events.clone();
        let partial = partial.clone();
        let tools = Arc::clone(&tools);
        let host = Arc::clone(&host);
        async move {
            let mut preamble = spec.preamble;
            let refs = describe_mentions(tools.as_ref(), &spec.mentions).await;
            if !refs.is_empty() {
                preamble.push_str("\n\n");
                preamble.push_str(&refs);
            }
            let input = LoopInput {
                preamble,
                history: spec.history,
            };
            run_turns(
                model.as_ref(),
                tools.as_ref(),
                host.as_ref(),
                input,
                &spec.config,
                &events,
                &partial,
            )
            .await
        }
    };

    match supervise(work, &token, limit).await {
        Supervised::Finished(Ok(LoopOutcome::Answered { text, tokens })) => {
            if text.trim().is_empty() {
                fail(
                    &events,
                    AgentErrorKind::Provider,
                    "The model returned an empty reply. Try again, or pick another model.",
                );
            } else {
                match host.save_reply(&text) {
                    Ok(message_id) => events.send(AgentEvent::Done { message_id, tokens }),
                    Err(err) => fail(&events, AgentErrorKind::Internal, err),
                }
            }
        }
        Supervised::Finished(Ok(LoopOutcome::Exhausted {
            text,
            turns,
            tokens,
        })) => {
            events.send(AgentEvent::Exhausted { turns });
            let note = format!(
                "_Stopped after {turns} steps without a final answer. Press Continue to keep going._"
            );
            match host.save_reply(&with_note(&text, &note)) {
                Ok(message_id) => events.send(AgentEvent::Done { message_id, tokens }),
                Err(err) => fail(&events, AgentErrorKind::Internal, err),
            }
        }
        Supervised::Finished(Err(err)) => {
            keep_partial(host.as_ref(), &partial, "_The model call failed here._");
            fail(&events, AgentErrorKind::Provider, err.message);
        }
        Supervised::Cancelled => {
            keep_partial(host.as_ref(), &partial, "_Stopped._");
            fail(&events, AgentErrorKind::Cancelled, "Stopped.");
        }
        Supervised::TimedOut => {
            keep_partial(host.as_ref(), &partial, "_Timed out._");
            fail(
                &events,
                AgentErrorKind::Timeout,
                format!(
                    "The model did not finish in {}s. Try again, or use a smaller model.",
                    limit.as_secs()
                ),
            );
        }
        Supervised::Panicked(err) => {
            tracing::error!(error = %err, "agent run task failed");
            fail(&events, AgentErrorKind::Internal, "The agent run failed.");
        }
    }
    drop(events);
    let _ = forwarder.await;
}
