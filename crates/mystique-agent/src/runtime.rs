use mystique_core::{
    AgentChat, AgentMessage, AppError, AppResult, AppServices, AppendAgentMessageInput,
    CreateAgentChatInput,
};
use rig::client::Nothing;
use rig::completion::{Chat, Message};
use rig::prelude::*;
use rig::providers::{anthropic, deepseek, gemini, ollama, openai, openrouter, xai};
use rig::{Agent, AgentBuilder};
use serde::{Deserialize, Serialize};

use crate::providers::{models_for, parse_provider, AgentModelInfo, AgentProvider};
use crate::secrets::SecretStore;
use crate::tools::{
    extract_leaked_tool_calls, strip_leaked_tool_calls, BoundTools, DescribeObject,
    ExecuteReadonly, GetSchemaDbml, LeakedToolCall, ListObjects, ListSavedQueries,
};

pub const SETTING_AI_ENABLED: &str = "ai.enabled";
pub const SETTING_OLLAMA_URL: &str = "ai.ollama.base_url";
pub const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendAgentMessage {
    pub connection_id: String,
    pub connection_name: String,
    pub engine: String,
    pub database: Option<String>,
    pub chat_id: Option<String>,
    pub provider: String,
    pub model: String,
    pub text: String,
    /// Live editor buffer for the active query tab. Not persisted as a chat message.
    #[serde(default)]
    pub editor_query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendAgentResult {
    pub chat: AgentChat,
    pub user: AgentMessage,
    pub assistant: AgentMessage,
}

#[derive(Clone, Default)]
pub struct AgentRuntime;

impl AgentRuntime {
    pub fn new() -> Self {
        Self
    }

    pub fn enabled(app: &AppServices) -> bool {
        app.settings()
            .get(SETTING_AI_ENABLED)
            .ok()
            .flatten()
            .map(|s| s.value == "true" || s.value == "1")
            .unwrap_or(false)
    }

    pub fn ollama_url(app: &AppServices) -> String {
        app.settings()
            .get(SETTING_OLLAMA_URL)
            .ok()
            .flatten()
            .map(|s| s.value)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_OLLAMA_URL.into())
    }

    /// Cloud providers use a static catalog. Ollama is listed from the local daemon.
    pub async fn list_models(
        &self,
        app: &AppServices,
        provider: AgentProvider,
    ) -> AppResult<Vec<AgentModelInfo>> {
        match provider {
            AgentProvider::Ollama => list_ollama_models(app).await,
            other => Ok(models_for(other)),
        }
    }

    pub async fn send(
        &self,
        app: &AppServices,
        secrets: &SecretStore,
        req: SendAgentMessage,
    ) -> AppResult<SendAgentResult> {
        if !Self::enabled(app) {
            return Err(AppError::Invalid(
                "AI agent is off. Enable it in Settings.".into(),
            ));
        }
        let text = req.text.trim().to_string();
        if text.is_empty() {
            return Err(AppError::Invalid("message is required".into()));
        }
        let provider = parse_provider(&req.provider)?;
        let model = req.model.trim();
        let model = if model.is_empty() {
            let fallback = provider.default_model();
            if fallback.is_empty() {
                return Err(AppError::Invalid(match provider {
                    AgentProvider::Ollama => {
                        "No Ollama model selected. Pull one (`ollama pull …`) and pick it.".into()
                    }
                    _ => "model is required".into(),
                }));
            }
            fallback.to_string()
        } else {
            model.to_string()
        };
        let database = req
            .database
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("")
            .to_string();

        let chat = match req
            .chat_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(id) => {
                let existing = app.agent_chats().get(id)?;
                if existing.connection_id != req.connection_id {
                    return Err(AppError::Invalid(
                        "chat does not belong to this connection".into(),
                    ));
                }
                app.agent_chats()
                    .update_meta(id, None, Some(provider.as_str()), Some(&model))?
            }
            None => {
                let title = title_from(&text);
                app.agent_chats().create(CreateAgentChatInput {
                    connection_id: req.connection_id.clone(),
                    title: Some(title),
                    provider: provider.as_str().into(),
                    model: model.clone(),
                })?
            }
        };

        let prior = app.agent_chats().list_messages(&chat.id)?;
        let user = app.agent_chats().append_message(AppendAgentMessageInput {
            chat_id: chat.id.clone(),
            role: "user".into(),
            content: text.clone(),
            tool_name: None,
            draft_body: None,
        })?;

        let editor = req.editor_query.clone().unwrap_or_default();
        let explain = looks_like_explain(&text, &editor);
        let tools = app.agent_tools();
        // Never preload schema (catalog + relationships + ping). That is 2–3
        // extra DB round-trips on every send and freezes the desktop UI.
        // Tools can fetch schema only when the model actually needs it.
        let preamble = system_prompt(
            &req.connection_name,
            &req.engine,
            &database,
            &editor,
            explain,
        );

        let bound = BoundTools::new(tools, req.connection_id.clone(), database);
        let limit = if explain {
            std::time::Duration::from_secs(45)
        } else {
            std::time::Duration::from_secs(AGENT_TIMEOUT_SECS)
        };
        // Run on another worker so a stuck HTTP call cannot block the timeout.
        let provider_run = provider;
        let model_run = model.clone();
        let text_run = text.clone();
        let preamble_run = preamble.clone();
        let secrets_run = secrets.clone();
        let app_url = AgentRuntime::ollama_url(app);
        let prior_run = prior.clone();
        let handle = tokio::spawn(async move {
            run_agent(
                provider_run,
                &model_run,
                &secrets_run,
                &app_url,
                &preamble_run,
                &prior_run,
                &text_run,
                bound,
                !explain,
            )
            .await
        });
        let reply = match tokio::time::timeout(limit, handle).await {
            Ok(Ok(Ok(reply))) => reply,
            Ok(Ok(Err(e))) => return Err(AppError::Internal(e)),
            Ok(Err(e)) => return Err(AppError::Internal(format!("agent task: {e}"))),
            Err(_) => {
                return Err(AppError::Invalid(
                    "The model did not finish in time. Try again, or use a smaller model.".into(),
                ));
            }
        };

        let draft = extract_draft(&reply);
        let assistant = app.agent_chats().append_message(AppendAgentMessageInput {
            chat_id: chat.id.clone(),
            role: "assistant".into(),
            content: reply,
            tool_name: None,
            draft_body: draft,
        })?;

        if prior.is_empty() {
            let _ = app
                .agent_chats()
                .update_meta(&chat.id, Some(&title_from(&text)), None, None);
        }

        let chat = app.agent_chats().get(&chat.id)?;
        Ok(SendAgentResult {
            chat,
            user,
            assistant,
        })
    }
}

fn title_from(text: &str) -> String {
    let one = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("Chat");
    let mut t = one.trim().to_string();
    if t.chars().count() > 48 {
        t = t.chars().take(45).collect::<String>() + "…";
    }
    t
}

const EDITOR_MAX: usize = 40_000;
/// Tool-loop budget: schema + sequential `describe_object` for several
/// tables + optional probes, then a fenced draft.
const DEFAULT_MAX_TURNS: usize = 16;
const AGENT_TIMEOUT_SECS: u64 = 180;

fn editor_fence(engine: &str) -> &'static str {
    match engine.to_ascii_lowercase().as_str() {
        "mongodb" | "mongo" | "mql" => "mql",
        _ => "sql",
    }
}

fn looks_like_explain(user: &str, editor: &str) -> bool {
    if editor.trim().is_empty() {
        return false;
    }
    let t = user.to_ascii_lowercase();
    const MUTATE: &[&str] = &[
        "rewrite",
        "write me",
        "write a",
        "fix",
        "change",
        "optimize",
        "generate",
        "draft",
        "replace",
        "add a ",
        "add the ",
        "remove ",
        "create a",
        "update the query",
        "make it",
    ];
    if MUTATE.iter().any(|s| t.contains(s)) {
        return false;
    }
    if t.contains('?') {
        return true;
    }
    const EXPLAIN: &[&str] = &[
        "describe",
        "explain",
        "what does",
        "what do",
        "what's this",
        "whats this",
        "what is this",
        "what is the query",
        "what this query",
        "mean",
        "walk through",
        "walk me through",
        "break down",
        "how does this",
        "how does the query",
        "tell me about",
        "summarize",
        "summary of",
    ];
    EXPLAIN.iter().any(|s| t.contains(s))
}

fn editor_block(engine: &str, raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "Current query tab editor: (empty). Only ask the user to paste a query if they refer to one and this block is empty.".into();
    }
    let fence = editor_fence(engine);
    let mut body = trimmed.to_string();
    let mut note = String::new();
    if body.len() > EDITOR_MAX {
        body.truncate(EDITOR_MAX);
        note = "\n… truncated.".into();
    }
    format!(
        "Current query tab editor (live buffer). If the user says \"this query\", \"the query\", or asks what it means, they mean this text. Do not ask them to paste it.\n```{fence}\n{body}{note}\n```"
    )
}

fn system_prompt(
    name: &str,
    engine: &str,
    database: &str,
    editor_query: &str,
    explain: bool,
) -> String {
    let db = if database.is_empty() {
        "(default catalog)".into()
    } else {
        database.to_string()
    };
    let editor = editor_block(engine, editor_query);
    if explain {
        return format!(
            "You are Mystique DB’s query assistant for connection `{name}`, engine `{engine}`, database `{db}`.\n\
             The user wants the current editor query explained. That query is below.\n\
             Answer in plain language: what it returns, main clauses, filters, joins, and caveats.\n\
             Do not call tools. Do not invent a new query unless they asked to change it.\n\
             {editor}"
        );
    }
    format!(
        "You are Mystique DB’s query assistant for connection `{name}`, engine `{engine}`, database `{db}`.\n\
         You can explain the current editor query or propose a replacement in a fenced ```sql or ```mql block.\n\
         Use schema-qualified names. Do not invent tables or columns.\n\
         Call get_schema_dbml or describe_object only if you must inspect schema. Prefer the editor buffer.\n\
         When a draft spans several tables, inspect every needed table in one turn (parallel describe_object calls). Do not spend a turn per table.\n\
         When you use a tool, use the native function/tool-calling interface. Never print a JSON object like {{\"name\":\"list_objects\",\"arguments\":{{}}}} in your reply.\n\
         Writes are rejected. Never ask for hosts, passwords, or schema pastes. The user applies the draft and runs it.\n\
         {editor}"
    )
}

fn history_from(prior: &[AgentMessage]) -> Vec<Message> {
    let mut out = Vec::new();
    let start = prior.len().saturating_sub(20);
    for msg in &prior[start..] {
        match msg.role.as_str() {
            "user" => out.push(Message::user(&msg.content)),
            "assistant" => out.push(Message::assistant(&msg.content)),
            _ => {}
        }
    }
    out
}

async fn run_agent(
    provider: AgentProvider,
    model: &str,
    secrets: &SecretStore,
    ollama_url: &str,
    preamble: &str,
    prior: &[AgentMessage],
    user: &str,
    bound: BoundTools,
    with_tools: bool,
) -> Result<String, String> {
    let mut history = history_from(prior);
    match provider {
        AgentProvider::Openai => {
            let key = require_key(secrets, provider)?;
            let client = openai::Client::new(&key).map_err(|e| e.to_string())?;
            chat_once(
                client.agent(model),
                preamble,
                bound,
                user,
                &mut history,
                with_tools,
            )
            .await
        }
        AgentProvider::Anthropic => {
            let key = require_key(secrets, provider)?;
            let client = anthropic::Client::new(&key).map_err(|e| e.to_string())?;
            chat_once(
                client.agent(model),
                preamble,
                bound,
                user,
                &mut history,
                with_tools,
            )
            .await
        }
        AgentProvider::Google => {
            let key = require_key(secrets, provider)?;
            let client = gemini::Client::new(&key).map_err(|e| e.to_string())?;
            chat_once(
                client.agent(model),
                preamble,
                bound,
                user,
                &mut history,
                with_tools,
            )
            .await
        }
        AgentProvider::Deepseek => {
            let key = require_key(secrets, provider)?;
            let client = deepseek::Client::new(&key).map_err(|e| e.to_string())?;
            chat_once(
                client.agent(model),
                preamble,
                bound,
                user,
                &mut history,
                with_tools,
            )
            .await
        }
        AgentProvider::Openrouter => {
            let key = require_key(secrets, provider)?;
            let client = openrouter::Client::new(&key).map_err(|e| e.to_string())?;
            chat_once(
                client.agent(model),
                preamble,
                bound,
                user,
                &mut history,
                with_tools,
            )
            .await
        }
        AgentProvider::Xai => {
            let key = require_key(secrets, provider)?;
            let client = xai::Client::new(&key).map_err(|e| e.to_string())?;
            chat_once(
                client.agent(model),
                preamble,
                bound,
                user,
                &mut history,
                with_tools,
            )
            .await
        }
        AgentProvider::Ollama => {
            let client = ollama::Client::builder()
                .api_key(Nothing)
                .base_url(ollama_url)
                .build()
                .map_err(|e| e.to_string())?;
            chat_once(
                client.agent(model),
                preamble,
                bound,
                user,
                &mut history,
                with_tools,
            )
            .await
        }
    }
}

async fn list_ollama_models(app: &AppServices) -> AppResult<Vec<AgentModelInfo>> {
    let url = AgentRuntime::ollama_url(app);
    let client = ollama::Client::builder()
        .api_key(Nothing)
        .base_url(&url)
        .build()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let listed = client.list_models().await.map_err(|e| {
        AppError::Invalid(format!(
            "Could not list Ollama models at {url}. Is Ollama running? ({e})"
        ))
    })?;

    let mut out: Vec<AgentModelInfo> = listed
        .iter()
        .filter(|m| !is_likely_embedding(&m.id) && !is_likely_embedding(m.display_name()))
        .map(|m| AgentModelInfo {
            id: m.id.clone(),
            label: m.display_name().to_string(),
        })
        .collect();
    if out.is_empty() {
        out = listed
            .iter()
            .map(|m| AgentModelInfo {
                id: m.id.clone(),
                label: m.display_name().to_string(),
            })
            .collect();
    }
    out.sort_by(|a, b| {
        a.label
            .to_ascii_lowercase()
            .cmp(&b.label.to_ascii_lowercase())
    });
    out.dedup_by(|a, b| a.id == b.id);
    Ok(out)
}

fn is_likely_embedding(id: &str) -> bool {
    let l = id.to_ascii_lowercase();
    l.contains("embed")
        || l.contains("minilm")
        || l.contains("nomic-bert")
        || l.contains("bge-")
        || l.contains("e5-")
}

fn require_key(secrets: &SecretStore, provider: AgentProvider) -> Result<String, String> {
    secrets
        .get(provider)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("No API key for {}. Add one in Settings.", provider.label()))
}

async fn chat_once(
    builder: AgentBuilder,
    preamble: &str,
    bound: BoundTools,
    user: &str,
    history: &mut Vec<Message>,
    with_tools: bool,
) -> Result<String, String> {
    if !with_tools {
        let agent: Agent = builder.preamble(preamble).max_tokens(2048).build();
        return agent
            .chat(Message::user(user), history)
            .await
            .map_err(|e| e.to_string());
    }
    let dispatcher = bound.clone();
    let agent: Agent = builder
        .preamble(preamble)
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

pub fn extract_draft(text: &str) -> Option<String> {
    for fence in ["```sql", "```mql", "```mongodb"] {
        if let Some(rest) = text.split_once(fence) {
            let body = rest.1.split_once("```").map(|(b, _)| b).unwrap_or(rest.1);
            let trimmed = body.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    if let Some(rest) = text.split_once("```") {
        let after = rest.1.trim_start();
        let after = after
            .strip_prefix("json")
            .or_else(|| after.strip_prefix("javascript"))
            .unwrap_or(after);
        let body = after.split_once("```").map(|(b, _)| b).unwrap_or(after);
        let trimmed = body.trim();
        if !trimmed.is_empty() && trimmed.len() < 20_000 {
            return Some(trimmed.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::looks_like_explain;

    #[test]
    fn explain_requires_editor_buffer() {
        assert!(!looks_like_explain("describe this query", ""));
        assert!(looks_like_explain("describe this query", "select 1 from t"));
    }

    #[test]
    fn explain_matches_common_phrasings() {
        let q = "select id from users";
        assert!(looks_like_explain("What does this query mean?", q));
        assert!(looks_like_explain("explain the sql", q));
        assert!(looks_like_explain("walk me through this", q));
        assert!(looks_like_explain("what about this?", q));
    }

    #[test]
    fn explain_ignores_rewrite_requests() {
        let q = "select id from users";
        assert!(!looks_like_explain(
            "describe this and then write a faster one",
            q
        ));
        assert!(!looks_like_explain("fix this query", q));
    }
}
