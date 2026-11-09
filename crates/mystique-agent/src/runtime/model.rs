//! One streamed model call, behind a small trait so the loop can be tested
//! with a fake provider.
//!
//! The real implementation wraps any Rig [`CompletionModel`]: it streams the
//! request, forwards text and reasoning deltas as they arrive, and returns
//! the aggregated assistant content (text, tool calls, reasoning with its
//! signatures) for the history. Dropping the future drops the HTTP request.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;

use futures::StreamExt;
use rig::client::Nothing;
use rig::completion::{
    AssistantContent, CompletionError, CompletionModel, CompletionResponse, Message,
    ToolDefinition, Usage,
};
use rig::prelude::*;
use rig::providers::{
    anthropic, cohere, deepseek, gemini, groq, huggingface, mistral, moonshot, ollama, openai,
    openrouter, together, xai,
};
use rig::streaming::StreamedAssistantContent;

use crate::providers::AgentProvider;
use crate::secrets::SecretStore;

pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What one model call is asked.
pub(crate) struct TurnRequest {
    pub preamble: String,
    /// Non-empty. The last message is the prompt for this call.
    pub history: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub max_tokens: u64,
}

/// A streamed piece of the reply.
pub(crate) enum Delta {
    Text(String),
    Reasoning(String),
}

/// The finished call: every content block, for the history.
pub(crate) struct TurnOutput {
    pub content: Vec<AssistantContent>,
    pub usage: Usage,
}

#[derive(Debug, Clone)]
pub(crate) struct TurnError {
    pub message: String,
    /// A transport or overload failure worth one more try, if nothing streamed.
    pub retryable: bool,
}

impl TurnError {
    pub(crate) fn fatal(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: false,
        }
    }
}

pub(crate) type DeltaSink<'a> = &'a (dyn Fn(Delta) + Send + Sync);

pub(crate) trait TurnModel: Send + Sync {
    fn turn<'a>(
        &'a self,
        req: TurnRequest,
        on_delta: DeltaSink<'a>,
    ) -> BoxFuture<'a, Result<TurnOutput, TurnError>>;
}

struct RigTurnModel<M> {
    model: M,
}

fn classify(err: CompletionError) -> TurnError {
    let message = err.to_string();
    let lower = message.to_ascii_lowercase();
    let retryable = matches!(err, CompletionError::HttpError(_))
        || [
            "429",
            "500",
            "502",
            "503",
            "504",
            "529",
            "overloaded",
            "rate limit",
            "timed out",
        ]
        .iter()
        .any(|needle| lower.contains(needle));
    TurnError { message, retryable }
}

impl<M> TurnModel for RigTurnModel<M>
where
    M: CompletionModel + Clone + 'static,
{
    fn turn<'a>(
        &'a self,
        req: TurnRequest,
        on_delta: DeltaSink<'a>,
    ) -> BoxFuture<'a, Result<TurnOutput, TurnError>> {
        Box::pin(async move {
            let mut history = req.history;
            let prompt = history
                .pop()
                .ok_or_else(|| TurnError::fatal("empty model request"))?;
            let mut stream = self
                .model
                .completion_request(prompt)
                .preamble(req.preamble)
                .messages(history)
                .tools(req.tools)
                .max_tokens(req.max_tokens)
                .stream()
                .await
                .map_err(classify)?;
            // A provider that sends whole reasoning blocks without deltas.
            let mut reasoning_seen: HashSet<String> = HashSet::new();
            while let Some(item) = stream.next().await {
                match item.map_err(classify)? {
                    StreamedAssistantContent::Text(text) => {
                        on_delta(Delta::Text(text.text().to_string()));
                    }
                    StreamedAssistantContent::ReasoningDelta { id, reasoning, .. } => {
                        reasoning_seen.insert(id);
                        on_delta(Delta::Reasoning(reasoning));
                    }
                    StreamedAssistantContent::Reasoning { reasoning, id }
                        if !reasoning_seen.contains(&id) =>
                    {
                        on_delta(Delta::Reasoning(reasoning.display_text()));
                    }
                    _ => {}
                }
            }
            let response: CompletionResponse = stream.into();
            Ok(TurnOutput {
                content: response.choice,
                usage: response.usage,
            })
        })
    }
}

fn boxed<M>(model: M) -> Box<dyn TurnModel>
where
    M: CompletionModel + Clone + 'static,
{
    Box::new(RigTurnModel { model })
}

fn require_key(secrets: &SecretStore, provider: AgentProvider) -> Result<String, String> {
    secrets
        .get(provider)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("No API key for {}. Add one in Settings.", provider.label()))
}

macro_rules! keyed {
    ($mod:ident, $secrets:expr, $provider:expr, $model:expr) => {{
        let key = require_key($secrets, $provider)?;
        let client = $mod::Client::new(&key).map_err(|e| e.to_string())?;
        Ok(boxed(client.completion_model($model)))
    }};
}

/// The streaming model for a provider. Secrets stay in the backend.
pub(crate) fn turn_model(
    provider: AgentProvider,
    model: &str,
    secrets: &SecretStore,
    ollama_url: &str,
) -> Result<Box<dyn TurnModel>, String> {
    match provider {
        AgentProvider::Openai => keyed!(openai, secrets, provider, model),
        AgentProvider::Anthropic => keyed!(anthropic, secrets, provider, model),
        AgentProvider::Google => keyed!(gemini, secrets, provider, model),
        AgentProvider::Groq => keyed!(groq, secrets, provider, model),
        AgentProvider::Together => keyed!(together, secrets, provider, model),
        AgentProvider::Mistral => keyed!(mistral, secrets, provider, model),
        AgentProvider::Cohere => keyed!(cohere, secrets, provider, model),
        AgentProvider::Huggingface => keyed!(huggingface, secrets, provider, model),
        AgentProvider::Moonshot => keyed!(moonshot, secrets, provider, model),
        AgentProvider::Deepseek => keyed!(deepseek, secrets, provider, model),
        AgentProvider::Openrouter => keyed!(openrouter, secrets, provider, model),
        AgentProvider::Xai => keyed!(xai, secrets, provider, model),
        AgentProvider::Ollama => {
            let client = ollama::Client::builder()
                .api_key(Nothing)
                .base_url(ollama_url)
                .build()
                .map_err(|e| e.to_string())?;
            Ok(boxed(client.completion_model(model)))
        }
    }
}
