use std::sync::Arc;

use mystique_agent::{
    default_model, parse_provider, provider_catalog, AgentModelInfo, AgentProviderInfo,
    AiKeyStatus, SecretStore, SendAgentMessage, SendAgentResult, DEFAULT_OLLAMA_URL,
    SETTING_AI_ENABLED, SETTING_OLLAMA_URL,
};
use mystique_core::{AgentChat, AgentMessage, AppServices, CreateAgentChatInput};
use serde::Deserialize;
use tauri::State;

use super::IpcError;
use crate::AppState;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsView {
    pub enabled: bool,
    pub ollama_base_url: String,
    /// True after the user saves an Ollama URL (even the default).
    pub ollama_configured: bool,
    pub keys: Vec<AiKeyStatus>,
    pub providers: Vec<AgentProviderInfo>,
}

fn read_ai_settings(
    services: &AppServices,
    secrets: &SecretStore,
) -> Result<AiSettingsView, IpcError> {
    let enabled = mystique_agent::runtime_enabled(services);
    let ollama_row = services
        .settings()
        .get(SETTING_OLLAMA_URL)
        .map_err(IpcError::from)?;
    let ollama_configured = ollama_row
        .as_ref()
        .map(|s| !s.value.trim().is_empty())
        .unwrap_or(false);
    let ollama_base_url = ollama_row
        .map(|s| s.value)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_OLLAMA_URL.into());
    Ok(AiSettingsView {
        enabled,
        ollama_base_url,
        ollama_configured,
        keys: secrets.status(),
        providers: provider_catalog(),
    })
}

#[tauri::command]
pub async fn get_ai_settings(state: State<'_, AppState>) -> Result<AiSettingsView, IpcError> {
    let services = Arc::clone(&state.services);
    let secrets = state.agent.secrets().clone();
    super::guard_blocking(move || read_ai_settings(&services, &secrets)).await
}

#[tauri::command]
pub async fn set_ai_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<AiSettingsView, IpcError> {
    let services = Arc::clone(&state.services);
    let secrets = state.agent.secrets().clone();
    super::guard_blocking(move || {
        services
            .settings()
            .set(SETTING_AI_ENABLED, if enabled { "true" } else { "false" })
            .map_err(IpcError::from)?;
        read_ai_settings(&services, &secrets)
    })
    .await
}

#[tauri::command]
pub async fn set_ai_ollama_url(
    state: State<'_, AppState>,
    url: String,
) -> Result<AiSettingsView, IpcError> {
    let services = Arc::clone(&state.services);
    let secrets = state.agent.secrets().clone();
    super::guard_blocking(move || {
        let url = url.trim();
        if url.is_empty() {
            let _ = services.settings().delete(SETTING_OLLAMA_URL);
        } else {
            services
                .settings()
                .set(SETTING_OLLAMA_URL, url)
                .map_err(IpcError::from)?;
        }
        read_ai_settings(&services, &secrets)
    })
    .await
}

#[tauri::command]
pub async fn set_ai_api_key(
    state: State<'_, AppState>,
    provider: String,
    key: String,
) -> Result<AiSettingsView, IpcError> {
    let services = Arc::clone(&state.services);
    let secrets = state.agent.secrets().clone();
    super::guard_blocking(move || {
        let p = parse_provider(&provider).map_err(IpcError::from)?;
        secrets.set(p, &key).map_err(IpcError::from)?;
        read_ai_settings(&services, &secrets)
    })
    .await
}

/// Static catalog. Stays sync: no keychain and no SQLite. See `ipc/mod.rs`.
#[tauri::command]
pub fn list_agent_providers() -> Result<Vec<AgentProviderInfo>, IpcError> {
    super::guard_sync(|| Ok(provider_catalog()))
}

#[tauri::command]
pub async fn list_agent_models(
    state: State<'_, AppState>,
    provider: String,
) -> Result<Vec<AgentModelInfo>, IpcError> {
    super::guard(async move {
        let p = parse_provider(&provider).map_err(IpcError::from)?;
        let services = std::sync::Arc::clone(&state.services);
        let runtime = state.agent.runtime().clone();
        runtime
            .list_models(&services, p)
            .await
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn list_agent_chats(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<AgentChat>, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .agent_chats()
            .list(&connection_id)
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn create_agent_chat(
    state: State<'_, AppState>,
    input: CreateAgentChatInput,
) -> Result<AgentChat, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        let provider = parse_provider(&input.provider).map_err(IpcError::from)?;
        let model = if input.model.trim().is_empty() {
            default_model(provider)
        } else {
            input.model
        };
        services
            .agent_chats()
            .create(CreateAgentChatInput {
                connection_id: input.connection_id,
                title: input.title,
                provider: provider.as_str().into(),
                model,
            })
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn delete_agent_chat(state: State<'_, AppState>, id: String) -> Result<bool, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || services.agent_chats().delete(&id).map_err(IpcError::from)).await
}

#[tauri::command]
pub async fn list_agent_messages(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<Vec<AgentMessage>, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .agent_chats()
            .list_messages(&chat_id)
            .map_err(IpcError::from)
    })
    .await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAgentChatInput {
    pub id: String,
    pub title: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[tauri::command]
pub async fn update_agent_chat(
    state: State<'_, AppState>,
    input: UpdateAgentChatInput,
) -> Result<AgentChat, IpcError> {
    let services = Arc::clone(&state.services);
    super::guard_blocking(move || {
        services
            .agent_chats()
            .update_meta(
                &input.id,
                input.title.as_deref(),
                input.provider.as_deref(),
                input.model.as_deref(),
            )
            .map_err(IpcError::from)
    })
    .await
}

#[tauri::command]
pub async fn send_agent_message(
    state: State<'_, AppState>,
    request: SendAgentMessage,
) -> Result<SendAgentResult, IpcError> {
    super::guard(async move {
        let services = std::sync::Arc::clone(&state.services);
        let secrets = state.agent.secrets().clone();
        let runtime = state.agent.runtime().clone();
        runtime
            .send(&services, &secrets, request)
            .await
            .map_err(IpcError::from)
    })
    .await
}
