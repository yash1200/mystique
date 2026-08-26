pub(crate) mod chat;
pub(crate) mod draft;
pub(crate) mod prompt;

use mystique_core::{
    AgentChat, AgentMessage, AppError, AppResult, AppServices, AppendAgentMessageInput,
    CreateAgentChatInput,
};
use rig::client::Nothing;
use rig::prelude::*;
use rig::providers::ollama;
use serde::{Deserialize, Serialize};

use crate::providers::{models_for, parse_provider, AgentModelInfo, AgentProvider};
use crate::secrets::SecretStore;
use crate::tools::BoundTools;

use self::chat::{run_agent, AGENT_TIMEOUT_SECS};
use self::draft::extract_draft;
use self::prompt::workspace_context;

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
        let tools = app.agent_tools();
        // Never preload schema (catalog + relationships + ping). That is 2–3
        // extra DB round-trips on every send and freezes the desktop UI.
        // Tools can fetch schema only when the model actually needs it.
        let context = workspace_context(&req.connection_name, &req.engine, &database, &editor);

        let bound = BoundTools::new(tools, req.connection_id.clone(), database);
        let limit = std::time::Duration::from_secs(AGENT_TIMEOUT_SECS);
        // Run on another worker so a stuck HTTP call cannot block the timeout.
        let provider_run = provider;
        let model_run = model.clone();
        let text_run = text.clone();
        let context_run = context;
        let secrets_run = secrets.clone();
        let app_url = AgentRuntime::ollama_url(app);
        let prior_run = prior.clone();
        let handle = tokio::spawn(async move {
            run_agent(
                provider_run,
                &model_run,
                &secrets_run,
                &app_url,
                &context_run,
                &prior_run,
                &text_run,
                bound,
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
