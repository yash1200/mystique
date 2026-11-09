//! The parts of the built-in agent another front-end reuses: the ACP client
//! (`mystique-acp`) runs an external agent in the same pane, so it opens
//! turns, saves tool cards, streams events and redacts exactly as the
//! built-in agent does.
//!
//! - [`AgentRuntime::open_turn`](crate::AgentRuntime::open_turn) claims the
//!   chat (one live run per chat) and saves the user message; Stop
//!   (`AgentRuntime::cancel`) cancels the claim's token.
//! - [`EventStream`] is the built-in agent's forwarder: deltas batched every
//!   [`DELTA_FLUSH`], every other event passed through
//!   [`AgentEvent::redacted`](crate::AgentEvent).
//! - [`ChatHost`] saves tool cards and the reply, and lists the run's
//!   requests in the approval dock.

use serde_json::Value as JsonValue;
use tokio::task::JoinHandle;

use mystique_core::{AgentTools, ToolCaller};

use crate::runtime::host::ToolFailure;
use crate::runtime::runs::describe_mentions;
use crate::runtime::stream::spawn_forwarder;
use crate::runtime::{context, prompt, turns};
use crate::tools::BoundTools;
use crate::{AgentEventSink, AgentTableRef};

pub use crate::runtime::events::{redact_args, redact_text};
pub use crate::runtime::stream::{EventTx, DELTA_FLUSH};
pub use crate::runtime::{ChatHost, RunClaim, TurnStart};
pub use tokio_util::sync::CancellationToken;

/// The run's event queue and the task that forwards it to the sink.
pub struct EventStream {
    tx: EventTx,
    forwarder: JoinHandle<()>,
}

impl EventStream {
    /// Start forwarding to `sink`, batching deltas every [`DELTA_FLUSH`].
    pub fn start(sink: AgentEventSink) -> Self {
        let (tx, forwarder) = spawn_forwarder(sink, DELTA_FLUSH);
        Self { tx, forwarder }
    }

    pub fn sender(&self) -> EventTx {
        self.tx.clone()
    }

    /// Flush what is buffered and wait until every event reached the sink.
    /// Clones from [`EventStream::sender`] must be dropped first.
    pub async fn finish(self) {
        drop(self.tx);
        let _ = self.forwarder.await;
    }
}

/// The tool card body: `{ args, result }` or `{ args, error }`.
pub fn tool_card(args: &JsonValue, outcome: Result<&JsonValue, &str>) -> String {
    context::tool_card(args, outcome)
}

/// One line for the tool card header, e.g. "12 columns" or the first error line.
pub fn summarize(outcome: Result<&JsonValue, &str>) -> String {
    match outcome {
        Ok(value) => turns::summarize(Ok(value)),
        Err(message) => turns::summarize(Err(&ToolFailure {
            message: message.to_string(),
            at: None,
        })),
    }
}

/// The task contract the built-in agent runs under. An external agent gets
/// the same rules, so both answer the same way.
pub const TASK_CONTRACT: &str = prompt::TASK_CONTRACT;

/// The `@table` references of a turn with their columns, as the built-in
/// agent's preamble lists them (at most eight tables). The lookups run as
/// `caller`, under its access levels, on the chat's connection and
/// database. Empty when there are no references.
pub async fn referenced_tables(
    tools: AgentTools,
    connection_id: &str,
    database: &str,
    mentions: &[AgentTableRef],
    caller: ToolCaller,
) -> String {
    if mentions.is_empty() {
        return String::new();
    }
    let bound = BoundTools::new(
        tools,
        connection_id.to_string(),
        database.trim().to_string(),
    )
    .with_caller(caller);
    describe_mentions(&bound, mentions).await
}

/// Connection, engine, database and the editor buffer, as the built-in
/// agent's preamble states them.
pub fn workspace_context(name: &str, engine: &str, database: &str, editor_query: &str) -> String {
    prompt::workspace_context(name, engine, database, editor_query)
}

/// Earlier turns as one block of text, at most `budget` characters.
pub fn transcript(prior: &[mystique_core::AgentMessage], budget: usize) -> String {
    context::transcript_from(prior, budget)
}
