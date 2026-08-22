import { invoke } from '@tauri-apps/api/core'
import { isTauri } from './tauri'

export type AgentProviderId =
	| 'openai'
	| 'anthropic'
	| 'google'
	| 'deepseek'
	| 'openrouter'
	| 'ollama'

export interface AgentProviderInfo {
	id: AgentProviderId
	label: string
	needsKey: boolean
	defaultModel: string
}

export interface AgentModelInfo {
	id: string
	label: string
}

export interface AiKeyStatus {
	provider: AgentProviderId
	hasKey: boolean
}

export interface AiSettingsView {
	enabled: boolean
	ollamaBaseUrl: string
	keys: AiKeyStatus[]
	providers: AgentProviderInfo[]
}

export interface AgentChat {
	id: string
	connectionId: string
	title: string
	provider: string
	model: string
	createdAt: string
	updatedAt: string
}

export interface AgentMessage {
	id: string
	chatId: string
	role: 'user' | 'assistant' | 'tool' | string
	content: string
	toolName?: string | null
	draftBody?: string | null
	createdAt: string
}

export interface SendAgentResult {
	chat: AgentChat
	user: AgentMessage
	assistant: AgentMessage
}

function requireDesktop() {
	if (!isTauri()) {
		throw new Error('Agent requires the desktop app (`yarn tauri dev`).')
	}
}

export async function getAiSettings(): Promise<AiSettingsView> {
	requireDesktop()
	return invoke('get_ai_settings')
}

export async function setAiEnabled(enabled: boolean): Promise<AiSettingsView> {
	requireDesktop()
	return invoke('set_ai_enabled', { enabled })
}

export async function setAiOllamaUrl(url: string): Promise<AiSettingsView> {
	requireDesktop()
	return invoke('set_ai_ollama_url', { url })
}

export async function setAiApiKey(
	provider: string,
	key: string,
): Promise<AiSettingsView> {
	requireDesktop()
	return invoke('set_ai_api_key', { provider, key })
}

export async function listAgentProviders(): Promise<AgentProviderInfo[]> {
	requireDesktop()
	return invoke('list_agent_providers')
}

export async function listAgentModels(
	provider: string,
): Promise<AgentModelInfo[]> {
	requireDesktop()
	return invoke('list_agent_models', { provider })
}

export async function listAgentChats(
	connectionId: string,
): Promise<AgentChat[]> {
	requireDesktop()
	return invoke('list_agent_chats', { connectionId })
}

export async function createAgentChat(input: {
	connectionId: string
	title?: string | null
	provider: string
	model: string
}): Promise<AgentChat> {
	requireDesktop()
	return invoke('create_agent_chat', { input })
}

export async function updateAgentChat(input: {
	id: string
	title?: string | null
	provider?: string | null
	model?: string | null
}): Promise<AgentChat> {
	requireDesktop()
	return invoke('update_agent_chat', { input })
}

export async function deleteAgentChat(id: string): Promise<boolean> {
	requireDesktop()
	return invoke('delete_agent_chat', { id })
}

export async function listAgentMessages(
	chatId: string,
): Promise<AgentMessage[]> {
	requireDesktop()
	return invoke('list_agent_messages', { chatId })
}

export async function sendAgentMessage(request: {
	connectionId: string
	connectionName: string
	engine: string
	database?: string | null
	chatId?: string | null
	provider: string
	model: string
	text: string
	editorQuery?: string | null
}): Promise<SendAgentResult> {
	requireDesktop()
	return invoke('send_agent_message', { request })
}
