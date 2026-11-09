//! The agent loop: stream a model call, run the tools it asks for, repeat
//! until it answers or the turn budget runs out.
//!
//! Rules it keeps:
//! - deltas go out as they arrive (batched by the forwarder);
//! - a provider failure is retried once, and only while nothing of that
//!   call has streamed;
//! - tool results are cut to head + tail for the model (the card keeps all);
//! - compaction runs before every call and keeps tool pairs whole;
//! - a failed tool call answers with the one repair prompt (statement,
//!   error, position);
//! - at most [`MAX_TURNS`] model calls, then `Exhausted`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rig::completion::message::{AssistantContent, ToolCall, ToolResultContent, UserContent};
use rig::completion::Message;
use serde_json::Value as JsonValue;

use crate::tools::{extract_leaked_tool_calls, strip_leaked_tool_calls, LeakedToolCall};

use super::context::{compact, head_tail, tool_card, CONTEXT_BUDGET_CHARS, TOOL_RESULT_MAX_CHARS};
use super::events::{redact_args, AgentEvent, AgentTokens};
use super::host::{RunHost, ToolFailure, ToolRunner};
use super::model::{Delta, TurnError, TurnModel, TurnRequest};
use super::prompt::{repair_prompt, Repair};
use super::stream::EventTx;

/// Model calls per run. Schema + several `describe_object` + probes + draft.
pub(crate) const MAX_TURNS: usize = 16;
/// Retries of one model call, only before anything of it streamed.
const MAX_RETRIES: usize = 1;
/// Rounds of "you printed tool calls as JSON" before we take the text as is.
const MAX_LEAK_REPAIRS: usize = 3;

pub(crate) struct LoopConfig {
    pub max_turns: usize,
    pub max_tokens: u64,
    pub retry_backoff: Duration,
    /// How often a tool call waiting on the dock is checked.
    pub approval_poll: Duration,
    pub context_budget: usize,
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            max_turns: MAX_TURNS,
            max_tokens: 4096,
            retry_backoff: Duration::from_millis(600),
            approval_poll: Duration::from_millis(200),
            context_budget: CONTEXT_BUDGET_CHARS,
        }
    }
}

/// Text streamed so far, readable after the run is aborted.
#[derive(Clone, Default)]
pub(crate) struct Partial(Arc<Mutex<String>>);

impl Partial {
    fn push(&self, text: &str) {
        if let Ok(mut s) = self.0.lock() {
            s.push_str(text);
        }
    }

    fn ends_open(&self) -> bool {
        self.0
            .lock()
            .map(|s| !s.is_empty() && !s.ends_with("\n\n"))
            .unwrap_or(false)
    }

    pub(crate) fn text(&self) -> String {
        self.0.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum LoopOutcome {
    Answered {
        text: String,
        tokens: AgentTokens,
    },
    Exhausted {
        text: String,
        turns: u32,
        tokens: AgentTokens,
    },
}

pub(crate) struct LoopInput {
    pub preamble: String,
    /// Earlier turns, then this run's user prompt last.
    pub history: Vec<Message>,
}

fn text_of(content: &[AssistantContent]) -> String {
    content
        .iter()
        .filter_map(|c| match c {
            AssistantContent::Text(t) => Some(t.text()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn tool_calls(content: &[AssistantContent]) -> Vec<ToolCall> {
    content
        .iter()
        .filter_map(|c| match c {
            AssistantContent::ToolCall(call) => Some(call.clone()),
            _ => None,
        })
        .collect()
}

/// One line for the tool card header, e.g. "12 columns" or the first error line.
pub(crate) fn summarize(result: Result<&JsonValue, &ToolFailure>) -> String {
    match result {
        Err(fail) => fail
            .message
            .lines()
            .next()
            .unwrap_or("failed")
            .chars()
            .take(200)
            .collect(),
        Ok(JsonValue::Object(map)) => {
            if map.get("_truncated").and_then(JsonValue::as_bool) == Some(true) {
                return "large result (shortened)".into();
            }
            for key in ["rows", "columns", "objects", "queries", "connections"] {
                if let Some(list) = map.get(key).and_then(JsonValue::as_array) {
                    return format!("{} {key}", list.len());
                }
            }
            if let Some(n) = map.get("row_count").or_else(|| map.get("rowCount")) {
                return format!("{n} rows");
            }
            if let Some(dbml) = map.get("dbml").and_then(JsonValue::as_str) {
                return format!("schema, {} lines", dbml.lines().count());
            }
            map.iter()
                .find_map(|(key, value)| value.as_array().map(|l| format!("{} {key}", l.len())))
                .unwrap_or_else(|| "done".into())
        }
        Ok(JsonValue::Array(list)) => format!("{} items", list.len()),
        Ok(_) => "done".into(),
    }
}

/// One model call, retried once while nothing of it has streamed.
async fn call_model(
    model: &dyn TurnModel,
    req: impl Fn() -> TurnRequest,
    events: &EventTx,
    partial: &Partial,
    cfg: &LoopConfig,
) -> Result<super::model::TurnOutput, TurnError> {
    let mut attempt = 0;
    loop {
        let streamed = AtomicBool::new(false);
        let on_delta = |delta: Delta| {
            match delta {
                Delta::Text(text) => {
                    if text.is_empty() {
                        return;
                    }
                    if !streamed.swap(true, Ordering::SeqCst) && partial.ends_open() {
                        // A new call's text starts its own paragraph.
                        partial.push("\n\n");
                        events.send(AgentEvent::TextDelta {
                            text: "\n\n".into(),
                        });
                    }
                    partial.push(&text);
                    events.send(AgentEvent::TextDelta { text });
                }
                Delta::Reasoning(text) => {
                    streamed.store(true, Ordering::SeqCst);
                    events.send(AgentEvent::ReasoningDelta { text });
                }
            }
        };
        match model.turn(req(), &on_delta).await {
            Ok(out) => return Ok(out),
            Err(err)
                if err.retryable && attempt < MAX_RETRIES && !streamed.load(Ordering::SeqCst) =>
            {
                attempt += 1;
                tracing::info!(
                    attempt,
                    "agent model call failed before streaming; retrying"
                );
                tokio::time::sleep(cfg.retry_backoff).await;
            }
            Err(err) => return Err(err),
        }
    }
}

/// Emit `ToolApprovalPending` for each dock request this call raises.
async fn watch_approvals(host: &dyn RunHost, events: &EventTx, call_id: &str, every: Duration) {
    let mut seen: Vec<String> = Vec::new();
    loop {
        tokio::time::sleep(every).await;
        for req in host.pending_approvals() {
            if seen.contains(&req.id) {
                continue;
            }
            seen.push(req.id.clone());
            events.send(AgentEvent::ToolApprovalPending {
                id: call_id.to_string(),
                approval_id: req.id,
                sql: req.sql,
                needed: req.needed,
            });
        }
    }
}

/// Run one tool call: events, card, approval watch, and what the model reads.
async fn run_tool(
    tools: &dyn ToolRunner,
    host: &dyn RunHost,
    events: &EventTx,
    cfg: &LoopConfig,
    id: &str,
    name: &str,
    args: JsonValue,
) -> String {
    let shown = redact_args(&args);
    tracing::debug!(tool = name, args = %shown, "agent tool call");
    events.send(AgentEvent::ToolCallStart {
        id: id.to_string(),
        name: name.to_string(),
        args: shown.clone(),
    });
    let outcome = tokio::select! {
        out = tools.call(name, args.clone()) => out,
        _ = watch_approvals(host, events, id, cfg.approval_poll) => unreachable!("the watch never ends"),
    };
    let card = match &outcome {
        Ok(value) => tool_card(&shown, Ok(value)),
        Err(fail) => tool_card(&shown, Err(&fail.message)),
    };
    let message_id = host.save_tool(name, &card);
    events.send(AgentEvent::ToolCallEnd {
        id: id.to_string(),
        ok: outcome.is_ok(),
        summary: summarize(outcome.as_ref()),
        message_id,
    });
    match outcome {
        Ok(value) => {
            let body = serde_json::to_string(&value).unwrap_or_else(|_| "{}".into());
            head_tail(&body, TOOL_RESULT_MAX_CHARS)
        }
        Err(fail) => {
            let statement = args
                .get("query")
                .or_else(|| args.get("sql"))
                .and_then(JsonValue::as_str);
            repair_prompt(&Repair {
                what: &format!("The tool call {name} failed"),
                statement,
                error: &fail.message,
                at: fail.at,
                extra: None,
            })
        }
    }
}

/// Run the loop to an answer or to the turn budget.
pub(crate) async fn run_turns(
    model: &dyn TurnModel,
    tools: &dyn ToolRunner,
    host: &dyn RunHost,
    input: LoopInput,
    cfg: &LoopConfig,
    events: &EventTx,
    partial: &Partial,
) -> Result<LoopOutcome, TurnError> {
    let definitions = tools.definitions();
    let mut history = input.history;
    let mut pinned = history.len().saturating_sub(1);
    let mut tokens = AgentTokens::default();
    let mut leak_repairs = 0;
    let mut trimmed_results = 0u32;

    for turn in 0..cfg.max_turns {
        let (pin, compaction) = compact(&mut history, pinned, cfg.context_budget);
        pinned = pin;
        if compaction.dropped_messages > 0 || trimmed_results > 0 {
            events.send(AgentEvent::ContextCompacted {
                dropped_messages: u32::try_from(compaction.dropped_messages).unwrap_or(u32::MAX),
                trimmed_results,
            });
            trimmed_results = 0;
        }

        let out = call_model(
            model,
            || TurnRequest {
                preamble: input.preamble.clone(),
                history: history.clone(),
                tools: definitions.clone(),
                max_tokens: cfg.max_tokens,
            },
            events,
            partial,
            cfg,
        )
        .await?;
        tokens.add(&out.usage);

        let calls = tool_calls(&out.content);
        if calls.is_empty() {
            let text = text_of(&out.content);
            let leaked: Vec<LeakedToolCall> = extract_leaked_tool_calls(&text);
            if leaked.is_empty() || leak_repairs >= MAX_LEAK_REPAIRS {
                let answer = if leaked.is_empty() {
                    partial.text()
                } else {
                    strip_leaked_tool_calls(&partial.text())
                };
                return Ok(LoopOutcome::Answered {
                    text: answer.trim().to_string(),
                    tokens,
                });
            }
            // A local model printed its tool calls. Run them and repair once more.
            leak_repairs += 1;
            let mut results = String::new();
            for (n, call) in leaked.into_iter().enumerate() {
                let id = format!("leaked-{turn}-{n}");
                let body = run_tool(tools, host, events, cfg, &id, &call.name, call.args).await;
                results.push_str(&format!("\n[{}]\n{}\n", call.name, body));
            }
            history.push(Message::Assistant {
                id: None,
                content: vec![AssistantContent::text(if text.trim().is_empty() {
                    "(tool calls)".to_string()
                } else {
                    text
                })],
            });
            history.push(Message::user(repair_prompt(&Repair {
                what: "You printed tool calls as JSON text instead of using the tool interface. They were run for you",
                statement: None,
                error: "Tool calls must use the native tool-calling interface.",
                at: None,
                extra: Some(&format!("Results:{results}")),
            })));
            continue;
        }

        // The call and its results enter the history together.
        let mut results = Vec::with_capacity(calls.len());
        for call in &calls {
            let body = run_tool(
                tools,
                host,
                events,
                cfg,
                call.id.as_str(),
                &call.function.name,
                call.function.arguments.clone(),
            )
            .await;
            if body.contains("characters omitted") {
                trimmed_results += 1;
            }
            results.push(UserContent::tool_result_for(
                call.id.clone(),
                call.provider.clone(),
                call.function.name.clone(),
                vec![ToolResultContent::text(body)],
            ));
        }
        history.push(Message::Assistant {
            id: None,
            content: out.content,
        });
        history.push(Message::User { content: results });
    }

    Ok(LoopOutcome::Exhausted {
        text: partial.text().trim().to_string(),
        turns: u32::try_from(cfg.max_turns).unwrap_or(u32::MAX),
        tokens,
    })
}
