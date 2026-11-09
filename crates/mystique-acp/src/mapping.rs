//! `session/update` → [`AgentEvent`]: the pane renders an external agent's
//! turn exactly like the built-in agent's.
//!
//! | ACP update                      | AgentEvent                          |
//! |---------------------------------|-------------------------------------|
//! | `agent_message_chunk` (text)    | `textDelta`                         |
//! | `agent_thought_chunk` (text)    | `reasoningDelta`                    |
//! | `tool_call`                     | `toolCallStart` (+ `toolCallEnd` if it arrives finished) |
//! | `tool_call_update` completed/failed | `toolCallEnd` (tool card saved) |
//! | `plan`                          | `reasoningDelta` (a checklist)      |
//! | anything else                   | nothing                             |
//!
//! Text passes through [`SafeStream`] so no secret leaves, even split
//! across chunks. Tool arguments, results and titles are scrubbed before
//! they are shown or saved.

use std::collections::HashMap;

use agent_client_protocol_schema::v1::{
    ContentBlock, Plan, PlanEntryStatus, SessionNotification, SessionUpdate, ToolCall,
    ToolCallContent, ToolCallStatus, ToolCallUpdate,
};
use mystique_agent::kit::{summarize, tool_card, ChatHost, EventTx};
use mystique_agent::AgentEvent;
use serde_json::Value as JsonValue;

use crate::permission::{identify, ToolIdentity};
use crate::scrub::{SafeStream, Scrubber};

struct OpenTool {
    identity: ToolIdentity,
    args: JsonValue,
    /// Text of the latest `content` the agent reported for the call.
    output: String,
    raw_output: Option<JsonValue>,
    ended: bool,
}

/// One prompt turn, as it streams.
pub(crate) struct Turn {
    scrub: Scrubber,
    events: EventTx,
    host: ChatHost,
    text: SafeStream,
    thought: SafeStream,
    /// The reply as the user saw it (secrets already masked).
    reply: String,
    tools: HashMap<String, OpenTool>,
    /// Tool call ids in the order they started.
    order: Vec<String>,
    last_plan: String,
    /// Dock requests already announced.
    announced: Vec<String>,
}

impl Turn {
    pub(crate) fn new(scrub: Scrubber, events: EventTx, host: ChatHost) -> Self {
        Self {
            text: SafeStream::new(scrub.clone()),
            thought: SafeStream::new(scrub.clone()),
            scrub,
            events,
            host,
            reply: String::new(),
            tools: HashMap::new(),
            order: Vec::new(),
            last_plan: String::new(),
            announced: Vec::new(),
        }
    }

    /// The reply so far, secrets masked.
    pub(crate) fn reply(&self) -> &str {
        &self.reply
    }

    /// The tool a permission request refers to, from its `tool_call`.
    pub(crate) fn identity_of(&self, call_id: &str) -> Option<&ToolIdentity> {
        self.tools.get(call_id).map(|t| &t.identity)
    }

    /// Handle a `session/update` notification. Updates for another session
    /// or that do not parse are skipped.
    pub(crate) fn on_notification(&mut self, session_id: &str, params: JsonValue) {
        let Ok(note) = serde_json::from_value::<SessionNotification>(params) else {
            tracing::debug!("acp: skipped a session/update Mystique does not read");
            return;
        };
        if note.session_id.0.as_ref() != session_id {
            return;
        }
        self.on_update(note.update);
    }

    pub(crate) fn on_update(&mut self, update: SessionUpdate) {
        match update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                if let Some(text) = block_text(&chunk.content) {
                    self.push_text(&text);
                }
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                if let Some(text) = block_text(&chunk.content) {
                    self.push_thought(&text);
                }
            }
            SessionUpdate::ToolCall(call) => self.start_tool(call),
            SessionUpdate::ToolCallUpdate(update) => self.update_tool(update),
            SessionUpdate::Plan(plan) => self.show_plan(&plan),
            _ => {}
        }
    }

    fn push_text(&mut self, chunk: &str) {
        self.flush_thought();
        let shown = self.text.push(chunk);
        self.emit_text(shown);
    }

    fn emit_text(&mut self, shown: String) {
        if shown.is_empty() {
            return;
        }
        self.reply.push_str(&shown);
        self.events.send(AgentEvent::TextDelta { text: shown });
    }

    fn push_thought(&mut self, chunk: &str) {
        let shown = self.thought.push(chunk);
        if !shown.is_empty() {
            self.events.send(AgentEvent::ReasoningDelta { text: shown });
        }
    }

    fn flush_thought(&mut self) {
        let rest = self.thought.flush();
        if !rest.is_empty() {
            self.events.send(AgentEvent::ReasoningDelta { text: rest });
        }
    }

    /// Release held-back text before a tool card, so the order stays right.
    fn flush_streams(&mut self) {
        self.flush_thought();
        let rest = self.text.flush();
        self.emit_text(rest);
    }

    fn show_plan(&mut self, plan: &Plan) {
        let mut lines = vec!["Plan:".to_string()];
        for entry in &plan.entries {
            let mark = match entry.status {
                PlanEntryStatus::Completed => "[x]",
                PlanEntryStatus::InProgress => "[~]",
                _ => "[ ]",
            };
            lines.push(format!("- {mark} {}", entry.content.trim()));
        }
        let text = self.scrub.secrets_only(&lines.join("\n"));
        if text == self.last_plan || plan.entries.is_empty() {
            return;
        }
        self.last_plan = text.clone();
        self.flush_thought();
        self.events.send(AgentEvent::ReasoningDelta {
            text: format!("\n{text}\n"),
        });
    }

    fn start_tool(&mut self, call: ToolCall) {
        let id = call.tool_call_id.0.to_string();
        if self.tools.contains_key(&id) {
            return;
        }
        self.flush_streams();
        let identity = identify(
            call.name.as_deref(),
            Some(call.title.as_str()),
            call.meta.as_ref(),
        );
        let identity = match identity {
            ToolIdentity::Other(label) => ToolIdentity::Other(self.scrub.text(&label)),
            mine => mine,
        };
        let args = self.scrub.json(
            call.raw_input
                .as_ref()
                .unwrap_or(&JsonValue::Object(Default::default())),
        );
        tracing::debug!(tool = %identity.label(), mystique = identity.is_mystique(), "acp tool call");
        self.events.send(AgentEvent::ToolCallStart {
            id: id.clone(),
            name: identity.label(),
            args: args.clone(),
        });
        self.tools.insert(
            id.clone(),
            OpenTool {
                identity,
                args,
                output: content_text(&call.content),
                raw_output: call.raw_output,
                ended: false,
            },
        );
        self.order.push(id.clone());
        match call.status {
            ToolCallStatus::Completed => self.end_tool(&id, true),
            ToolCallStatus::Failed => self.end_tool(&id, false),
            _ => {}
        }
    }

    fn update_tool(&mut self, update: ToolCallUpdate) {
        let id = update.tool_call_id.0.to_string();
        if !self.tools.contains_key(&id) {
            // An update for a call we never saw start: start it now.
            let title = update.fields.title.clone().unwrap_or_else(|| "tool".into());
            let mut call = ToolCall::new(id.clone(), title);
            call.name = update.fields.name.clone();
            call.raw_input = update.fields.raw_input.clone();
            call.meta = update.meta.clone();
            self.start_tool(call);
        }
        let Some(tool) = self.tools.get_mut(&id) else {
            return;
        };
        if let Some(content) = &update.fields.content {
            tool.output = content_text(content);
        }
        if let Some(raw) = update.fields.raw_output {
            tool.raw_output = Some(raw);
        }
        match update.fields.status {
            Some(ToolCallStatus::Completed) => self.end_tool(&id, true),
            Some(ToolCallStatus::Failed) => self.end_tool(&id, false),
            _ => {}
        }
    }

    fn end_tool(&mut self, id: &str, ok: bool) {
        let Some(tool) = self.tools.get_mut(id) else {
            return;
        };
        if tool.ended {
            return;
        }
        tool.ended = true;
        let result = self
            .scrub
            .json(&result_value(tool.raw_output.as_ref(), &tool.output));
        let (card, summary) = if ok {
            (tool_card(&tool.args, Ok(&result)), summarize(Ok(&result)))
        } else {
            let message = match &result {
                JsonValue::String(s) if !s.trim().is_empty() => s.clone(),
                JsonValue::Null => "The tool call failed.".to_string(),
                other => other.to_string(),
            };
            (
                tool_card(&tool.args, Err(&message)),
                summarize(Err(&message)),
            )
        };
        let name = tool.identity.label();
        let message_id = self.host.save_tool(&name, &card);
        self.events.send(AgentEvent::ToolCallEnd {
            id: id.to_string(),
            ok,
            summary,
            message_id,
        });
    }

    /// The newest of Mystique's tool calls still running, if any.
    fn open_mystique_call(&self) -> Option<&str> {
        self.order
            .iter()
            .rev()
            .find(|id| {
                self.tools
                    .get(id.as_str())
                    .is_some_and(|t| !t.ended && t.identity.is_mystique())
            })
            .map(String::as_str)
    }

    /// Announce dock requests raised by this turn's running Mystique call.
    pub(crate) fn check_approvals(&mut self) {
        let Some(call) = self.open_mystique_call().map(str::to_owned) else {
            return;
        };
        for req in self.host.pending_approvals() {
            if self.announced.contains(&req.id) {
                continue;
            }
            self.announced.push(req.id.clone());
            self.events.send(AgentEvent::ToolApprovalPending {
                id: call.clone(),
                approval_id: req.id,
                sql: req.sql,
                needed: req.needed,
            });
        }
    }

    /// The turn is over: release held text and close calls left open.
    pub(crate) fn finish(&mut self) {
        self.flush_streams();
        let open: Vec<String> = self
            .order
            .iter()
            .filter(|id| self.tools.get(id.as_str()).is_some_and(|t| !t.ended))
            .cloned()
            .collect();
        for id in open {
            if let Some(tool) = self.tools.get_mut(&id) {
                if tool.output.is_empty() && tool.raw_output.is_none() {
                    tool.output = "The agent did not report a result.".into();
                }
            }
            self.end_tool(&id, false);
        }
    }
}

fn block_text(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text(t) => Some(t.text.clone()),
        ContentBlock::ResourceLink(link) => Some(format!("[{}]({})", link.name, link.uri)),
        _ => None,
    }
}

fn content_text(content: &[ToolCallContent]) -> String {
    content
        .iter()
        .filter_map(|c| match c {
            ToolCallContent::Content(inner) => block_text(&inner.content),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A tool result as JSON: the MCP `structuredContent` when the agent passed
/// the raw result on, else the text it showed (parsed when it is JSON).
fn result_value(raw: Option<&JsonValue>, text: &str) -> JsonValue {
    if let Some(raw) = raw {
        if let Some(structured) = raw.get("structuredContent") {
            return structured.clone();
        }
        if !raw.is_null() {
            return raw.clone();
        }
    }
    let text = text.trim();
    if text.is_empty() {
        return JsonValue::Null;
    }
    serde_json::from_str(text).unwrap_or_else(|_| JsonValue::String(text.to_string()))
}
