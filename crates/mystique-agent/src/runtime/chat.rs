use mystique_core::AgentMessage;
use rig::client::Nothing;
use rig::completion::{Chat, Message};
use rig::prelude::*;
use rig::providers::{
    anthropic, cohere, deepseek, gemini, groq, huggingface, mistral, moonshot, ollama, openai,
    openrouter, together, xai,
};
use rig::{Agent, AgentBuilder};

use crate::providers::AgentProvider;
use crate::secrets::SecretStore;
use crate::tools::{
    extract_leaked_tool_calls, strip_leaked_tool_calls, BoundTools, DescribeObject,
    ExecuteReadonly, GetSchemaDbml, LeakedToolCall, ListObjects, ListSavedQueries,
};

use super::prompt::SYSTEM_PROMPT;

/// Tool-loop budget: schema + sequential `describe_object` for several
/// tables + optional probes, then a fenced draft.
const DEFAULT_MAX_TURNS: usize = 16;
pub(super) const AGENT_TIMEOUT_SECS: u64 = 180;

pub(crate) fn history_from(prior: &[AgentMessage]) -> Vec<Message> {
    let mut out = Vec::new();
    let start = prior.len().saturating_sub(20);
    for msg in prior.get(start..).unwrap_or(&[]) {
        match msg.role.as_str() {
            "user" => out.push(Message::user(&msg.content)),
            "assistant" => out.push(Message::assistant(&msg.content)),
            _ => {}
        }
    }
    out
}

macro_rules! keyed_chat {
    ($mod:ident, $secrets:expr, $provider:expr, $model:expr, $bound:expr, $user:expr, $history:expr, $context:expr) => {{
        let key = require_key($secrets, $provider)?;
        let client = $mod::Client::new(&key).map_err(|e| e.to_string())?;
        chat_once(client.agent($model), $bound, $user, &mut $history, $context).await
    }};
}

pub(super) async fn run_agent(
    provider: AgentProvider,
    model: &str,
    secrets: &SecretStore,
    ollama_url: &str,
    context: &str,
    prior: &[AgentMessage],
    user: &str,
    bound: BoundTools,
) -> Result<String, String> {
    let mut history = history_from(prior);
    match provider {
        AgentProvider::Openai => {
            keyed_chat!(openai, secrets, provider, model, bound, user, history, context)
        }
        AgentProvider::Anthropic => {
            keyed_chat!(anthropic, secrets, provider, model, bound, user, history, context)
        }
        AgentProvider::Google => {
            keyed_chat!(gemini, secrets, provider, model, bound, user, history, context)
        }
        AgentProvider::Groq => {
            keyed_chat!(groq, secrets, provider, model, bound, user, history, context)
        }
        AgentProvider::Together => {
            keyed_chat!(together, secrets, provider, model, bound, user, history, context)
        }
        AgentProvider::Mistral => {
            keyed_chat!(mistral, secrets, provider, model, bound, user, history, context)
        }
        AgentProvider::Cohere => {
            keyed_chat!(cohere, secrets, provider, model, bound, user, history, context)
        }
        AgentProvider::Huggingface => {
            keyed_chat!(
                huggingface,
                secrets,
                provider,
                model,
                bound,
                user,
                history,
                context
            )
        }
        AgentProvider::Moonshot => {
            keyed_chat!(moonshot, secrets, provider, model, bound, user, history, context)
        }
        AgentProvider::Deepseek => {
            keyed_chat!(deepseek, secrets, provider, model, bound, user, history, context)
        }
        AgentProvider::Openrouter => {
            keyed_chat!(openrouter, secrets, provider, model, bound, user, history, context)
        }
        AgentProvider::Xai => {
            keyed_chat!(xai, secrets, provider, model, bound, user, history, context)
        }
        AgentProvider::Ollama => {
            let client = ollama::Client::builder()
                .api_key(Nothing)
                .base_url(ollama_url)
                .build()
                .map_err(|e| e.to_string())?;
            chat_once(client.agent(model), bound, user, &mut history, context).await
        }
    }
}

fn require_key(secrets: &SecretStore, provider: AgentProvider) -> Result<String, String> {
    secrets
        .get(provider)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("No API key for {}. Add one in Settings.", provider.label()))
}

async fn chat_once(
    builder: AgentBuilder,
    bound: BoundTools,
    user: &str,
    history: &mut Vec<Message>,
    context: &str,
) -> Result<String, String> {
    let dispatcher = bound.clone();
    let agent: Agent = builder
        .preamble(SYSTEM_PROMPT)
        .context(context)
        .tool(GetSchemaDbml(bound.clone()))
        .tool(ListObjects(bound.clone()))
        .tool(DescribeObject(bound.clone()))
        .tool(ExecuteReadonly(bound.clone()))
        .tool(ListSavedQueries(bound))
        .default_max_turns(DEFAULT_MAX_TURNS)
        .max_tokens(4096)
        .build();
    let mut prompt = user.to_string();
    for turn in 0..4 {
        let reply = agent
            .chat(Message::user(&prompt), history)
            .await
            .map_err(|e| e.to_string())?;
        let leaked = extract_leaked_tool_calls(&reply);
        if leaked.is_empty() {
            return Ok(reply);
        }
        if turn == 3 {
            let cleaned = strip_leaked_tool_calls(&reply);
            if cleaned.trim().is_empty() {
                return Err("The local model printed tool calls instead of answering. \
                     Try a tool-capable Ollama model (qwen2.5, llama3.1, or a larger variant)."
                    .into());
            }
            return Ok(cleaned);
        }
        prompt = execute_leaked_tools(&dispatcher, leaked).await;
    }
    Err("The local model printed tool calls instead of answering.".into())
}

async fn execute_leaked_tools(bound: &BoundTools, calls: Vec<LeakedToolCall>) -> String {
    let mut out = String::from(
        "The previous assistant message was a tool invocation printed as JSON, not an answer. \
         Those tools have been executed. Use the results below. \
         Reply to the user with a draft query in a ```sql or ```mql fence. \
         Do not print {\"name\": ...} JSON again.\n",
    );
    for call in calls {
        match bound.dispatch(&call.name, call.args).await {
            Ok(value) => {
                let mut body = serde_json::to_string(&value).unwrap_or_else(|_| "{}".into());
                if body.len() > 12_000 {
                    body.truncate(12_000);
                    body.push('…');
                }
                out.push_str(&format!("\n[{}]\n{}\n", call.name, body));
            }
            Err(err) => {
                out.push_str(&format!("\n[{}] error: {err}\n", call.name));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::history_from;
    use mystique_core::AgentMessage;
    use rig::completion::Message;

    fn prior(role: &str, content: &str) -> AgentMessage {
        AgentMessage {
            id: "m1".into(),
            chat_id: "c1".into(),
            role: role.into(),
            content: content.into(),
            tool_name: None,
            draft_body: None,
            created_at: String::new(),
        }
    }

    #[test]
    fn history_is_prior_turns_only() {
        let history = history_from(&[prior("user", "hello"), prior("assistant", "hi")]);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].rag_text().as_deref(), Some("hello"));
        assert!(
            matches!(history[1], Message::Assistant { .. }),
            "expected assistant turn, got {:?}",
            history[1]
        );
    }
}
