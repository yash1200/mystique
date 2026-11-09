//! What the model sees, kept inside a budget.
//!
//! - A tool result over [`TOOL_RESULT_MAX_CHARS`] is cut to its head and tail
//!   for the model only; the tool card in SQLite keeps all of it.
//! - The conversation is cut from the oldest end to [`CONTEXT_BUDGET_CHARS`].
//!   An assistant message that calls tools and the user message holding
//!   their results are one unit: they go together or not at all.
//! - Earlier turns' tool cards are folded into that turn's assistant text,
//!   so replayed history never carries tool ids a provider did not issue.

use mystique_core::AgentMessage;
use rig::completion::message::{AssistantContent, UserContent};
use rig::completion::Message;
use serde_json::Value as JsonValue;

/// Larger tool results reach the model as head + tail.
pub(crate) const TOOL_RESULT_MAX_CHARS: usize = 12_000;
/// A tool card replayed from an earlier turn.
const PRIOR_TOOL_MAX_CHARS: usize = 2_000;
/// The whole conversation the model is sent (characters, not tokens).
pub(crate) const CONTEXT_BUDGET_CHARS: usize = 96_000;
/// Earlier messages read back from the chat.
const PRIOR_MESSAGES: usize = 40;

/// `text` if it fits, else its head and tail around a marker the model can act on.
pub(crate) fn head_tail(text: &str, max: usize) -> String {
    let total = text.chars().count();
    if total <= max {
        return text.to_string();
    }
    let head_len = max * 2 / 3;
    let tail_len = max.saturating_sub(head_len);
    let head: String = text.chars().take(head_len).collect();
    let tail: String = text.chars().skip(total - tail_len).collect();
    let omitted = total - head_len - tail_len;
    format!(
        "{head}\n…[{omitted} characters omitted. Ask for fewer rows or columns, or page with cell_char_offset.]…\n{tail}"
    )
}

fn content_chars(msg: &Message) -> usize {
    match msg {
        Message::System { content } => content.chars().count(),
        Message::User { content } => content
            .iter()
            .map(|c| match c {
                UserContent::Text(t) => t.text().chars().count(),
                UserContent::ToolResult(r) => r
                    .content
                    .iter()
                    .map(|part| serde_json::to_string(part).map(|s| s.len()).unwrap_or(0))
                    .sum(),
                _ => 256,
            })
            .sum(),
        Message::Assistant { content, .. } => content
            .iter()
            .map(|c| match c {
                AssistantContent::Text(t) => t.text().chars().count(),
                AssistantContent::ToolCall(call) => {
                    call.function.name.len() + call.function.arguments.to_string().len()
                }
                AssistantContent::Reasoning(r) => r.display_text().chars().count(),
                _ => 256,
            })
            .sum(),
    }
}

fn calls_tools(msg: &Message) -> bool {
    matches!(msg, Message::Assistant { content, .. }
        if content.iter().any(|c| matches!(c, AssistantContent::ToolCall(_))))
}

fn holds_results(msg: &Message) -> bool {
    matches!(msg, Message::User { content }
        if content.iter().any(|c| matches!(c, UserContent::ToolResult(_))))
}

/// `[start, end)` ranges: a tool call and its results are one unit.
fn units(history: &[Message]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < history.len() {
        let pair = history.get(i).is_some_and(calls_tools)
            && history.get(i + 1).is_some_and(holds_results);
        let end = if pair { i + 2 } else { i + 1 };
        out.push((i, end));
        i = end;
    }
    out
}

/// What [`compact`] did, for `ContextCompacted`.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Compaction {
    pub dropped_messages: usize,
}

/// Drop the oldest units until the history fits `budget`. `pinned` (the
/// run's user prompt) and the newest unit always stay. Returns the new index
/// of `pinned`.
pub(crate) fn compact(
    history: &mut Vec<Message>,
    pinned: usize,
    budget: usize,
) -> (usize, Compaction) {
    let mut pinned = pinned;
    let mut report = Compaction::default();
    loop {
        let total: usize = history.iter().map(content_chars).sum();
        let all = units(history);
        let starts_with_reply = matches!(history.first(), Some(Message::Assistant { .. }));
        if total <= budget && !starts_with_reply {
            break;
        }
        let last = all.len().saturating_sub(1);
        let victim = all
            .iter()
            .enumerate()
            .find(|(n, (start, end))| *n != last && !(*start <= pinned && pinned < *end))
            .map(|(_, unit)| *unit);
        let Some((start, end)) = victim else { break };
        if total <= budget && start != 0 {
            // Only a leading reply was left to drop, and it is not first.
            break;
        }
        history.drain(start..end);
        report.dropped_messages += end - start;
        if start < pinned {
            pinned -= end - start;
        }
    }
    (pinned, report)
}

/// The tool card body a run stores: `{ args, result }` or `{ args, error }`.
pub(crate) fn tool_card(args: &JsonValue, outcome: Result<&JsonValue, &str>) -> String {
    let body = match outcome {
        Ok(result) => serde_json::json!({ "args": args, "result": result }),
        Err(error) => serde_json::json!({ "args": args, "error": error }),
    };
    serde_json::to_string_pretty(&body).unwrap_or_else(|_| "{}".into())
}

fn prior_tool_line(row: &AgentMessage) -> String {
    let name = row.tool_name.as_deref().unwrap_or("tool");
    let parsed: Option<JsonValue> = serde_json::from_str(&row.content).ok();
    let (args, body) = match &parsed {
        Some(JsonValue::Object(map)) if map.contains_key("args") => {
            let args = map.get("args").map(|a| a.to_string()).unwrap_or_default();
            let body = map
                .get("result")
                .map(|r| r.to_string())
                .or_else(|| map.get("error").map(|e| format!("error: {e}")))
                .unwrap_or_default();
            (args, body)
        }
        _ => (String::new(), row.content.clone()),
    };
    format!(
        "- {name}({args}) → {}",
        head_tail(&body, PRIOR_TOOL_MAX_CHARS)
    )
}

/// Earlier turns as plain user/assistant messages. Tool cards that came
/// before an assistant reply are folded into it as a short note.
pub(crate) fn history_from(prior: &[AgentMessage]) -> Vec<Message> {
    let start = prior.len().saturating_sub(PRIOR_MESSAGES);
    let mut out = Vec::new();
    let mut tools: Vec<String> = Vec::new();
    let flush_tools = |tools: &mut Vec<String>, reply: Option<&str>| -> Option<Message> {
        if tools.is_empty() {
            return reply.map(Message::assistant);
        }
        let mut text = format!("(Tools I used for this answer:\n{})", tools.join("\n"));
        tools.clear();
        if let Some(reply) = reply {
            text.push_str("\n\n");
            text.push_str(reply);
        }
        Some(Message::assistant(text))
    };
    for msg in prior.get(start..).unwrap_or(&[]) {
        match msg.role.as_str() {
            "user" => {
                if let Some(m) = flush_tools(&mut tools, None) {
                    out.push(m);
                }
                out.push(Message::user(&msg.content));
            }
            "assistant" => {
                if let Some(m) = flush_tools(&mut tools, Some(&msg.content)) {
                    out.push(m);
                }
            }
            "tool" => tools.push(prior_tool_line(msg)),
            _ => {}
        }
    }
    if let Some(m) = flush_tools(&mut tools, None) {
        out.push(m);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::completion::message::{ToolCall, ToolFunction, ToolResultContent};
    use serde_json::json;

    fn row(role: &str, content: &str, tool: Option<&str>) -> AgentMessage {
        AgentMessage {
            id: "m".into(),
            chat_id: "c".into(),
            role: role.into(),
            content: content.into(),
            tool_name: tool.map(Into::into),
            draft_body: None,
            created_at: String::new(),
        }
    }

    fn tool_call(id: &str) -> Message {
        Message::Assistant {
            id: None,
            content: vec![AssistantContent::ToolCall(ToolCall::from_wire(
                id,
                ToolFunction::new("describe_object".into(), json!({ "name": "t" })),
            ))],
        }
    }

    fn tool_result(id: &str, body: &str) -> Message {
        Message::User {
            content: vec![UserContent::tool_result_from_wire(
                id,
                "describe_object",
                vec![ToolResultContent::text(body)],
            )],
        }
    }

    #[test]
    fn head_tail_keeps_both_ends_within_budget() {
        let text = format!("HEAD{}TAIL", "x".repeat(50_000));
        let out = head_tail(&text, TOOL_RESULT_MAX_CHARS);
        assert!(out.starts_with("HEAD"));
        assert!(out.ends_with("TAIL"));
        assert!(out.contains("characters omitted"));
        assert!(out.chars().count() < TOOL_RESULT_MAX_CHARS + 200);
        assert_eq!(head_tail("short", 10), "short");
        // Multi-byte text never splits a char.
        let wide = "你".repeat(20_000);
        assert!(head_tail(&wide, 100).starts_with("你"));
    }

    #[test]
    fn compaction_never_splits_a_tool_pair() {
        let big = "y".repeat(40_000);
        let mut history = vec![
            Message::user("old question"),
            Message::assistant("old answer"),
            Message::user("current question"),
            tool_call("a"),
            tool_result("a", &big),
            tool_call("b"),
            tool_result("b", &big),
            tool_call("c"),
            tool_result("c", &big),
        ];
        let (pinned, report) = compact(&mut history, 2, 90_000);
        assert!(report.dropped_messages > 0);
        assert!(matches!(history.first(), Some(Message::User { .. })));
        // Every remaining tool call is directly followed by its result.
        for (i, m) in history.iter().enumerate() {
            if calls_tools(m) {
                assert!(history.get(i + 1).is_some_and(holds_results), "{i}");
            }
            if holds_results(m) {
                assert!(i > 0 && calls_tools(&history[i - 1]), "{i}");
            }
        }
        assert_eq!(
            history[pinned].rag_text().as_deref(),
            Some("current question")
        );
        // Oldest pair first: `a` went, `c` (the newest) stayed.
        let total: usize = history.iter().map(content_chars).sum();
        assert!(total <= 90_000 || history.len() <= 3, "{total}");
    }

    #[test]
    fn compaction_is_a_no_op_under_budget() {
        let mut history = vec![Message::user("q"), tool_call("a"), tool_result("a", "{}")];
        let (pinned, report) = compact(&mut history, 0, CONTEXT_BUDGET_CHARS);
        assert_eq!(pinned, 0);
        assert_eq!(report, Compaction::default());
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn prior_tool_cards_fold_into_the_reply() {
        let card = tool_card(
            &json!({ "name": "orders" }),
            Ok(&json!({ "columns": [1, 2] })),
        );
        let history = history_from(&[
            row("user", "hello", None),
            row("tool", &card, Some("describe_object")),
            row("assistant", "hi", None),
        ]);
        assert_eq!(history.len(), 2);
        let Message::Assistant { content, .. } = &history[1] else {
            panic!("expected the reply");
        };
        let text = match content.first() {
            Some(AssistantContent::Text(t)) => t.text().to_string(),
            other => panic!("{other:?}"),
        };
        assert!(text.contains("describe_object"), "{text}");
        assert!(text.ends_with("hi"), "{text}");
    }

    #[test]
    fn history_is_prior_turns_only() {
        let history = history_from(&[row("user", "hello", None), row("assistant", "hi", None)]);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].rag_text().as_deref(), Some("hello"));
        assert!(matches!(history[1], Message::Assistant { .. }));
    }
}
