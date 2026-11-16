//! One streamed model call, behind a small trait so the loop can be tested
//! with a fake provider.
//!
//! The real implementation wraps a Rig completion model (`DynModel`): it
//! streams the request, forwards text and reasoning deltas as they arrive,
//! and returns the finished assistant turn (text, tool calls, reasoning with
//! its provider items) for the history. Dropping the future drops the HTTP
//! request.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;

use futures::StreamExt;
use rig::completion::{
    AssistantContent, AssistantMessage, CompletionRequest, Message, ToolDefinition, Usage,
};
use rig::operation::Completion;
use rig::providers::{
    anthropic, cohere, deepseek, gemini, groq, huggingface, mistral, moonshot, ollama, openai,
    openrouter, together, xai,
};
use rig::streaming::{Item, StreamEvent};
use rig::{DynModel, ProviderError};

use crate::providers::AgentProvider;
use crate::secrets::SecretStore;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What one model call is asked.
pub struct TurnRequest {
    pub preamble: String,
    /// Non-empty. The last message is the prompt for this call.
    pub history: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub max_tokens: u64,
}

/// A streamed piece of the reply.
pub enum Delta {
    Text(String),
    Reasoning(String),
}

/// The finished call. The whole assistant turn goes back into the history:
/// its origin and each block's provider item (reasoning signatures, call ids)
/// are what a provider needs to accept the conversation again.
pub struct TurnOutput {
    pub message: AssistantMessage,
    pub usage: Usage,
}

#[derive(Debug, Clone)]
pub struct TurnError {
    pub message: String,
    /// A transport or overload failure worth one more try, if nothing streamed.
    pub retryable: bool,
}

pub type DeltaSink<'a> = &'a (dyn Fn(Delta) + Send + Sync);

pub trait TurnModel: Send + Sync {
    fn turn<'a>(
        &'a self,
        req: TurnRequest,
        on_delta: DeltaSink<'a>,
    ) -> BoxFuture<'a, Result<TurnOutput, TurnError>>;
}

struct RigTurnModel {
    model: DynModel<Completion>,
    /// Display name for error messages ("Gemini").
    provider: &'static str,
}

/// The user sees one sentence; the raw provider text goes to the log. Rig
/// decides what is worth retrying (transport faults, 429/5xx, overload,
/// a stalled stream).
fn classify(provider: &str, err: ProviderError) -> TurnError {
    let raw = err.to_string();
    tracing::warn!(provider, error = %raw, "model call failed");
    TurnError {
        message: super::friendly::friendly_error(provider, &raw),
        retryable: err.is_retryable(),
    }
}

impl TurnModel for RigTurnModel {
    fn turn<'a>(
        &'a self,
        req: TurnRequest,
        on_delta: DeltaSink<'a>,
    ) -> BoxFuture<'a, Result<TurnOutput, TurnError>> {
        Box::pin(async move {
            let request = CompletionRequest::from(req.history)
                .preamble(req.preamble)
                .tools(req.tools)
                .max_tokens(req.max_tokens);
            let mut stream = self
                .model
                .stream(request)
                .map_err(|e| classify(self.provider, e))?;
            // Some providers send a reasoning block whole, with no deltas.
            let mut reasoning_streamed = HashSet::new();
            while let Some(item) = stream.next().await {
                let Item::Event(event) = item.map_err(|e| classify(self.provider, e))? else {
                    continue;
                };
                match event {
                    StreamEvent::Text { text, .. } => on_delta(Delta::Text(text)),
                    StreamEvent::Reasoning { part, text } => {
                        reasoning_streamed.insert(part);
                        on_delta(Delta::Reasoning(text));
                    }
                    StreamEvent::End {
                        part,
                        content: AssistantContent::Reasoning(reasoning),
                    } if !reasoning_streamed.contains(&part) => {
                        on_delta(Delta::Reasoning(reasoning.text));
                    }
                    _ => {}
                }
            }
            let response = stream
                .finish()
                .await
                .map_err(|e| classify(self.provider, e))?;
            let message = match response.message() {
                Some(Message::Assistant(message)) => message,
                _ => AssistantMessage::new(Vec::new()).with_origin(response.origin.clone()),
            };
            // A turn the provider failed in-band (a content filter, an error
            // stop) with nothing usable in it.
            if let Some(reason) = response.error.as_deref() {
                if message.text().trim().is_empty() && message.tool_calls().next().is_none() {
                    return Err(classify(
                        self.provider,
                        ProviderError::Provider(reason.into()),
                    ));
                }
            }
            Ok(TurnOutput {
                message,
                usage: response.usage,
            })
        })
    }
}

/// The adapter around one Rig completion model (`provider` names it in errors).
pub fn boxed(
    provider: AgentProvider,
    model: impl Into<DynModel<Completion>>,
) -> Box<dyn TurnModel> {
    Box::new(RigTurnModel {
        model: model.into(),
        provider: provider.label(),
    })
}

fn require_key(secrets: &SecretStore, provider: AgentProvider) -> Result<String, String> {
    secrets
        .get(provider)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("No API key for {}. Add one in Settings.", provider.label()))
}

/// `$client(key).completion(model)`: each provider's own client, or an
/// OpenAI-compatible vendor's `new`.
macro_rules! keyed {
    ($client:path, $secrets:expr, $provider:expr, $model:expr) => {{
        let key = require_key($secrets, $provider)?;
        Ok(boxed($provider, $client(key).completion($model)))
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
        AgentProvider::Openai => keyed!(openai::OpenAI::new, secrets, provider, model),
        AgentProvider::Anthropic => keyed!(anthropic::Anthropic::new, secrets, provider, model),
        AgentProvider::Google => keyed!(gemini::Gemini::new, secrets, provider, model),
        AgentProvider::Groq => keyed!(groq::new, secrets, provider, model),
        AgentProvider::Together => keyed!(together::new, secrets, provider, model),
        AgentProvider::Mistral => keyed!(mistral::new, secrets, provider, model),
        AgentProvider::Cohere => keyed!(cohere::Cohere::new, secrets, provider, model),
        AgentProvider::Huggingface => keyed!(huggingface::new, secrets, provider, model),
        AgentProvider::Moonshot => keyed!(moonshot::new, secrets, provider, model),
        AgentProvider::Deepseek => keyed!(deepseek::new, secrets, provider, model),
        AgentProvider::Openrouter => keyed!(openrouter::new, secrets, provider, model),
        AgentProvider::Xai => keyed!(xai::new, secrets, provider, model),
        // The native `/api/chat`, as before: it takes `think` and `num_ctx`.
        AgentProvider::Ollama => Ok(boxed(
            provider,
            ollama::OllamaConfig::new()
                .with_base_url(ollama_url)
                .client()
                .native_completion(model),
        )),
    }
}
