use mystique_agent::{
    default_model, parse_provider, provider_catalog, AgentModelInfo, AgentProviderInfo,
    AiKeyStatus, SendAgentMessage, SendAgentResult, DEFAULT_OLLAMA_URL, SETTING_AI_ENABLED,
    SETTING_OLLAMA_URL,
};
use mystique_core::{AgentChat, AgentMessage, CreateAgentChatInput};
use serde::Deserialize;
use tauri::State;

use crate::AppState;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettingsView {
    pub enabled: bool,
    pub ollama_base_url: String,
    pub keys: Vec<AiKeyStatus>,
    pub providers: Vec<AgentProviderInfo>,
}

#[tauri::command]
pub fn get_ai_settings(state: State<'_, AppState>) -> Result<AiSettingsView, String> {
    let enabled = mystique_agent::runtime_enabled(&state.services);
    let ollama_base_url = state
        .services
        .settings()
        .get(SETTING_OLLAMA_URL)
        .map_err(|e| e.to_string())?
        .map(|s| s.value)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_OLLAMA_URL.into());
    Ok(AiSettingsView {
        enabled,
        ollama_base_url,
        keys: state.agent.secrets().status(),
        providers: provider_catalog(),
    })
}

#[tauri::command]
pub fn set_ai_enabled(state: State<'_, AppState>, enabled: bool) -> Result<AiSettingsView, String> {
    state
        .services
        .settings()
        .set(SETTING_AI_ENABLED, if enabled { "true" } else { "false" })
        .map_err(|e| e.to_string())?;
    get_ai_settings(state)
}

#[tauri::command]
pub fn set_ai_ollama_url(
    state: State<'_, AppState>,
    url: String,
) -> Result<AiSettingsView, String> {
    let url = url.trim();
    if url.is_empty() {
        let _ = state.services.settings().delete(SETTING_OLLAMA_URL);
    } else {
        state
            .services
            .settings()
            .set(SETTING_OLLAMA_URL, url)
            .map_err(|e| e.to_string())?;
    }
    get_ai_settings(state)
}

#[tauri::command]
pub fn set_ai_api_key(
    state: State<'_, AppState>,
    provider: String,
    key: String,
) -> Result<AiSettingsView, String> {
    let p = parse_provider(&provider).map_err(|e| e.to_string())?;
    state
        .agent
        .secrets()
        .set(p, &key)
        .map_err(|e| e.to_string())?;
    get_ai_settings(state)
}

#[tauri::command]
pub fn list_agent_providers() -> Vec<AgentProviderInfo> {
    provider_catalog()
}

#[tauri::command]
pub async fn list_agent_models(
    state: State<'_, AppState>,
    provider: String,
) -> Result<Vec<AgentModelInfo>, String> {
    let p = parse_provider(&provider).map_err(|e| e.to_string())?;
    let services = std::sync::Arc::clone(&state.services);
    let runtime = state.agent.runtime().clone();
    runtime
        .list_models(&services, p)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_agent_chats(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<AgentChat>, String> {
    state
        .services
        .agent_chats()
        .list(&connection_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_agent_chat(
    state: State<'_, AppState>,
    input: CreateAgentChatInput,
) -> Result<AgentChat, String> {
    let provider = parse_provider(&input.provider).map_err(|e| e.to_string())?;
    let model = if input.model.trim().is_empty() {
        default_model(provider)
    } else {
        input.model
    };
    state
        .services
        .agent_chats()
        .create(CreateAgentChatInput {
            connection_id: input.connection_id,
            title: input.title,
            provider: provider.as_str().into(),
            model,
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_agent_chat(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    state
        .services
        .agent_chats()
        .delete(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_agent_messages(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<Vec<AgentMessage>, String> {
    state
        .services
        .agent_chats()
        .list_messages(&chat_id)
        .map_err(|e| e.to_string())
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
pub fn update_agent_chat(
    state: State<'_, AppState>,
    input: UpdateAgentChatInput,
) -> Result<AgentChat, String> {
    state
        .services
        .agent_chats()
        .update_meta(
            &input.id,
            input.title.as_deref(),
            input.provider.as_deref(),
            input.model.as_deref(),
        )
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn send_agent_message(
    state: State<'_, AppState>,
    request: SendAgentMessage,
) -> Result<SendAgentResult, String> {
    let services = std::sync::Arc::clone(&state.services);
    let secrets = state.agent.secrets().clone();
    let runtime = state.agent.runtime().clone();
    runtime
        .send(&services, &secrets, request)
        .await
        .map_err(|e| e.to_string())
}
