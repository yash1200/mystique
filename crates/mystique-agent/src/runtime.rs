pub(crate) mod context;
pub(crate) mod draft;
pub(crate) mod events;
pub(crate) mod host;
#[cfg(test)]
mod loop_tests;
pub(crate) mod model;
pub(crate) mod ollama;
pub(crate) mod prompt;
pub(crate) mod runs;
pub(crate) mod stream;
pub(crate) mod turns;

use std::sync::Arc;

use mystique_core::{
    AgentChat, AgentMessage, AppError, AppResult, AppServices, AppendAgentMessageInput,
    CreateAgentChatInput,
};
use rig::completion::Message;
use serde::{Deserialize, Serialize};

use crate::providers::{models_for, parse_provider, AgentModelInfo, AgentProvider};
use crate::secrets::SecretStore;
use crate::tools::BoundTools;

pub use self::events::{AgentErrorKind, AgentEvent, AgentTokens};
pub use self::stream::AgentEventSink;

use self::context::history_from;
use self::host::AppHost;
use self::model::turn_model;
use self::ollama::list_ollama_models;
use self::prompt::{repair_prompt, workspace_context, ErrorAt, Repair, TASK_CONTRACT};
use self::runs::{execute_run, RunRegistry, RunSpec, RUN_TIME_LIMIT};
use self::turns::LoopConfig;

pub const SETTING_AI_ENABLED: &str = "ai.enabled";
pub const SETTING_OLLAMA_URL: &str = "ai.ollama.base_url";
pub const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";

/// A table the user referenced with `@` or dragged in from the explorer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct AgentTableRef {
    #[serde(default)]
    pub schema: Option<String>,
    pub name: String,
}

/// "Fix with AI" from the query error banner: the failed statement, the
/// error, and where it points (1-based), for the repair prompt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields = nullable))]
#[serde(rename_all = "camelCase")]
pub struct AgentFixRequest {
    pub sql: String,
    pub error: String,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub column: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
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
    /// `@table` references: their columns are looked up and sent with the turn.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<AgentTableRef>>", optional))]
    pub mentions: Vec<AgentTableRef>,
    /// Set by "Fix with AI": the turn opens with the repair prompt.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub fix: Option<AgentFixRequest>,
}

/// The run started: the chat (new or existing) and the saved user message.
/// Everything else arrives on the event channel, ending in `done` or `error`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SendAgentResult {
    pub chat: AgentChat,
    pub user: AgentMessage,
}

/// Long-lived: clones share the live-run registry.
#[derive(Clone, Default)]
pub struct AgentRuntime {
    runs: Arc<RunRegistry>,
}

impl AgentRuntime {
    pub fn new() -> Self {
        Self::default()
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

    /// Stop the chat's live run: the provider request is dropped and no
    /// further tool call runs. False when nothing was running.
    pub fn cancel(&self, chat_id: &str) -> bool {
        self.runs.cancel(chat_id.trim())
    }

    /// True while the chat has a run in flight.
    pub fn is_running(&self, chat_id: &str) -> bool {
        self.runs.is_live(chat_id.trim())
    }

    /// Save the user message and start the run. Returns at once; the reply
    /// streams to `sink` and ends with `done` or `error`. Must be called
    /// inside a Tokio runtime.
    pub fn send(
        &self,
        app: Arc<AppServices>,
        secrets: &SecretStore,
        req: SendAgentMessage,
        sink: AgentEventSink,
    ) -> AppResult<SendAgentResult> {
        if !Self::enabled(&app) {
            return Err(AppError::Invalid(
                "AI agent is off. Enable it in Settings.".into(),
            ));
        }
        let typed = req.text.trim().to_string();
        let text = match &req.fix {
            Some(fix) => fix_prompt(&typed, fix),
            None => typed.clone(),
        };
        if text.is_empty() {
            return Err(AppError::Invalid("message is required".into()));
        }
        let provider = parse_provider(&req.provider)?;
        require_configured(&app, secrets, provider)?;
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
        let streaming = turn_model(provider, &model, secrets, &Self::ollama_url(&app))
            .map_err(AppError::Invalid)?;

        let busy = || AppError::Invalid("This chat is still answering. Stop it or wait.".into());
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
                if self.runs.is_live(id) {
                    return Err(busy());
                }
                app.agent_chats()
                    .update_meta(id, None, Some(provider.as_str()), Some(&model))?
            }
            None => app.agent_chats().create(CreateAgentChatInput {
                connection_id: req.connection_id.clone(),
                title: Some(title_from(if typed.is_empty() { &text } else { &typed })),
                provider: provider.as_str().into(),
                model: model.clone(),
            })?,
        };
        let Some((run, token)) = self.runs.begin(&chat.id) else {
            return Err(busy());
        };

        let started = (|| -> AppResult<(Vec<AgentMessage>, AgentMessage)> {
            let prior = app.agent_chats().list_messages(&chat.id)?;
            let user = app.agent_chats().append_message(AppendAgentMessageInput {
                chat_id: chat.id.clone(),
                role: "user".into(),
                content: text.clone(),
                tool_name: None,
                draft_body: None,
            })?;
            Ok((prior, user))
        })();
        let (prior, user) = match started {
            Ok(rows) => rows,
            Err(err) => {
                self.runs.finish(&chat.id, run);
                return Err(err);
            }
        };

        // Never preload the whole schema here: that is several DB round
        // trips per send. `@table` references are looked up inside the run.
        let editor = req.editor_query.clone().unwrap_or_default();
        let preamble = format!(
            "{TASK_CONTRACT}\n\n{}",
            workspace_context(&req.connection_name, &req.engine, &database, &editor)
        );
        let mut history = history_from(&prior);
        history.push(Message::user(&text));
        let spec = RunSpec {
            preamble,
            mentions: req.mentions.clone(),
            history,
            config: LoopConfig::default(),
            limit: RUN_TIME_LIMIT,
        };
        let tools = Arc::new(BoundTools::new(
            app.agent_tools(),
            req.connection_id.clone(),
            database,
        ));
        let host = Arc::new(AppHost {
            app: Arc::clone(&app),
            chat_id: chat.id.clone(),
            connection_id: req.connection_id.clone(),
        });
        let runs = Arc::clone(&self.runs);
        let chat_id = chat.id.clone();
        tokio::spawn(async move {
            execute_run(streaming, tools, host, spec, token, sink).await;
            runs.finish(&chat_id, run);
        });

        let chat = app.agent_chats().get(&chat.id).unwrap_or(chat);
        Ok(SendAgentResult { chat, user })
    }
}

/// The user's words, then the repair prompt for the failed editor query.
fn fix_prompt(typed: &str, fix: &AgentFixRequest) -> String {
    let at = match (fix.line, fix.column) {
        (Some(line), Some(column)) if line > 0 && column > 0 => Some(ErrorAt { line, column }),
        _ => None,
    };
    let repair = repair_prompt(&Repair {
        what: "The query in the editor failed",
        statement: Some(&fix.sql),
        error: &fix.error,
        at,
        extra: None,
    });
    let ask = if typed.is_empty() {
        "Fix this query."
    } else {
        typed
    };
    format!("{ask}\n\n{repair}")
}

/// Refuse a run before we persist a chat against a provider the user never set up.
fn require_configured(
    app: &AppServices,
    secrets: &SecretStore,
    provider: AgentProvider,
) -> AppResult<()> {
    if matches!(provider, AgentProvider::Ollama) {
        let configured = app
            .settings()
            .get(SETTING_OLLAMA_URL)
            .ok()
            .flatten()
            .map(|s| !s.value.trim().is_empty())
            .unwrap_or(false);
        if !configured {
            return Err(AppError::Invalid(
                "Ollama is not set up. Add it in Settings.".into(),
            ));
        }
        return Ok(());
    }
    secrets.get(provider)?.ok_or_else(|| {
        AppError::Invalid(format!(
            "No API key for {}. Add one in Settings.",
            provider.label()
        ))
    })?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn harness() -> (Arc<AppServices>, SecretStore) {
        let app = Arc::new(AppServices::with_secrets(
            mystique_core::AppDatabase::open_in_memory().unwrap(),
            mystique_core::ConnectionManager::bootstrap(),
            mystique_core::SecretStore::memory(),
        ));
        let secrets = SecretStore::new(app.clone());
        (app, secrets)
    }

    #[test]
    fn require_configured_rejects_cloud_provider_without_key() {
        let (app, secrets) = harness();
        let err = require_configured(&app, &secrets, AgentProvider::Openai).unwrap_err();
        assert!(err.to_string().contains("No API key for OpenAI"));
    }

    #[test]
    fn require_configured_accepts_cloud_provider_with_key() {
        let (app, secrets) = harness();
        secrets.set(AgentProvider::Google, "test-key").unwrap();
        require_configured(&app, &secrets, AgentProvider::Google).unwrap();
    }

    #[test]
    fn require_configured_rejects_ollama_until_url_is_saved() {
        let (app, secrets) = harness();
        let err = require_configured(&app, &secrets, AgentProvider::Ollama).unwrap_err();
        assert!(err.to_string().contains("Ollama is not set up"));
        app.settings()
            .set(SETTING_OLLAMA_URL, DEFAULT_OLLAMA_URL)
            .unwrap();
        require_configured(&app, &secrets, AgentProvider::Ollama).unwrap();
    }
}
