use std::sync::Arc;

use mystique_acp::{
    AcpAgentSaved, AcpProbe, AcpSettingsView, DetectedAcpAgent, McpAccess, SaveAcpAgent,
    ACP_PROVIDER,
};
use mystique_agent::{
    parse_provider, provider_catalog, AgentEvent, AgentEventSink, AgentModelInfo, AgentRuntime,
    AiSettingsView, SecretStore, SendAgentMessage, SendAgentResult, DEFAULT_OLLAMA_URL,
    SETTING_AI_ENABLED, SETTING_OLLAMA_URL,
};
use mystique_core::{AgentChat, AgentMessage, AppServices, UpdateAgentChatInput};
use tauri::ipc::Channel;
use tauri::State;

use super::IpcError;
use crate::AppState;

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

/// Start a turn. Returns the chat and the saved user message at once; the
/// reply streams on `on_event` (batched deltas, tool calls, `done`/`error`).
/// `provider: "acp"` runs the external agent from Settings → AI instead of
/// the built-in one; the events and Stop are the same.
#[tauri::command]
pub async fn send_agent_message(
    state: State<'_, AppState>,
    request: SendAgentMessage,
    on_event: Channel<AgentEvent>,
) -> Result<SendAgentResult, IpcError> {
    let services = Arc::clone(&state.services);
    let secrets = state.agent.secrets().clone();
    let runtime = state.agent.runtime().clone();
    let sink: AgentEventSink = Arc::new(move |event| {
        let _ = on_event.send(event);
    });
    if request.provider.trim() == ACP_PROVIDER {
        return send_external(&state, runtime, request, sink).await;
    }
    // SQLite writes happen before the run starts; keep them off the worker.
    super::guard_blocking(move || {
        runtime
            .send(services, &secrets, request, sink)
            .map_err(IpcError::from)
    })
    .await
}

/// An external-agent turn. The agent reaches the databases through the
/// running MCP server, with its token.
async fn send_external(
    state: &State<'_, AppState>,
    runtime: AgentRuntime,
    request: SendAgentMessage,
    sink: AgentEventSink,
) -> Result<SendAgentResult, IpcError> {
    let acp = state.acp.clone();
    let mcp = state.mcp.clone();
    super::guard(async move {
        let access = match mcp.running_endpoint().await.map_err(IpcError::from)? {
            Some(endpoint) => McpAccess {
                url: endpoint.url,
                token: endpoint.token,
            },
            None => McpAccess {
                url: String::new(),
                token: String::new(),
            },
        };
        super::guard_blocking(move || {
            acp.send(&runtime, access, request, sink)
                .map_err(IpcError::from)
        })
        .await
    })
    .await
}

/// Settings → AI → Agent: the added external (ACP) agents and the default.
#[tauri::command]
pub async fn get_acp_settings(state: State<'_, AppState>) -> Result<AcpSettingsView, IpcError> {
    let acp = state.acp.clone();
    super::guard_blocking(move || Ok(acp.settings())).await
}

/// Add or change an agent (the Custom agent form). Secret-looking values go
/// to the vault.
#[tauri::command]
pub async fn save_acp_agent(
    state: State<'_, AppState>,
    input: SaveAcpAgent,
) -> Result<AcpAgentSaved, IpcError> {
    let acp = state.acp.clone();
    super::guard_blocking(move || acp.save_agent(input).map_err(IpcError::from)).await
}

/// Remove an agent and its vault secrets.
#[tauri::command]
pub async fn remove_acp_agent(
    state: State<'_, AppState>,
    id: String,
) -> Result<AcpSettingsView, IpcError> {
    let acp = state.acp.clone();
    super::guard_blocking(move || acp.remove_agent(&id).map_err(IpcError::from)).await
}

/// What new chats use: `builtin` or an agent id.
#[tauri::command]
pub async fn set_default_agent(
    state: State<'_, AppState>,
    id: String,
) -> Result<AcpSettingsView, IpcError> {
    let acp = state.acp.clone();
    super::guard_blocking(move || acp.set_default(&id).map_err(IpcError::from)).await
}

/// Which known ACP agents are on this machine (Rescan). Never installs
/// anything; asks each CLI found for its version (1.5 s limit each).
#[tauri::command]
pub async fn detect_acp_agents(
    state: State<'_, AppState>,
) -> Result<Vec<DetectedAcpAgent>, IpcError> {
    let acp = state.acp.clone();
    super::guard_blocking(move || Ok(acp.detect())).await
}

/// Add a detected agent with one click (its launch command as found here).
#[tauri::command]
pub async fn add_detected_acp_agent(
    state: State<'_, AppState>,
    catalog_id: String,
) -> Result<AcpAgentSaved, IpcError> {
    let acp = state.acp.clone();
    super::guard_blocking(move || acp.add_detected(&catalog_id).map_err(IpcError::from)).await
}

/// Test button: start agent `id`, run `initialize` and a sign-in check, stop it.
#[tauri::command]
pub async fn test_acp_agent(state: State<'_, AppState>, id: String) -> Result<AcpProbe, IpcError> {
    let acp = state.acp.clone();
    super::guard(async move { acp.probe(&id).await.map_err(IpcError::from) }).await
}

/// Stop the chat's live turn (Stop button). The provider request and any
/// tool call in flight are dropped. False when nothing was running.
#[tauri::command]
pub async fn cancel_agent_message(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<bool, IpcError> {
    let runtime = state.agent.runtime().clone();
    super::guard(async move { Ok(runtime.cancel(&chat_id)) }).await
}
