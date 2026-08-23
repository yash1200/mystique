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

function asId(value: unknown): string {
	if (value == null) return ''
	return String(value)
}

function normalizeChat(raw: AgentChat): AgentChat {
	return {
		...raw,
		id: asId(raw.id),
		connectionId: asId(raw.connectionId),
	}
}

function normalizeMessage(raw: AgentMessage): AgentMessage {
	return {
		...raw,
		id: asId(raw.id),
		chatId: asId(raw.chatId),
	}
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
	const rows = await invoke<AgentChat[]>('list_agent_chats', {
		connectionId: asId(connectionId),
	})
	return rows.map(normalizeChat)
}

export async function createAgentChat(input: {
	connectionId: string
	title?: string | null
	provider: string
	model: string
}): Promise<AgentChat> {
	requireDesktop()
	return normalizeChat(
		await invoke('create_agent_chat', {
			input: { ...input, connectionId: asId(input.connectionId) },
		}),
	)
}

export async function updateAgentChat(input: {
	id: string
	title?: string | null
	provider?: string | null
	model?: string | null
}): Promise<AgentChat> {
	requireDesktop()
	return normalizeChat(
		await invoke('update_agent_chat', {
			input: { ...input, id: asId(input.id) },
		}),
	)
}

export async function deleteAgentChat(id: string): Promise<boolean> {
	requireDesktop()
	return invoke('delete_agent_chat', { id: asId(id) })
}

export async function listAgentMessages(
	chatId: string,
): Promise<AgentMessage[]> {
	requireDesktop()
	const rows = await invoke<AgentMessage[]>('list_agent_messages', {
		chatId: asId(chatId),
	})
	return rows.map(normalizeMessage)
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
	const result = await invoke<SendAgentResult>('send_agent_message', {
		request: {
			...request,
			connectionId: asId(request.connectionId),
			chatId: request.chatId ? asId(request.chatId) : request.chatId,
		},
	})
	return {
		chat: normalizeChat(result.chat),
		user: normalizeMessage(result.user),
		assistant: normalizeMessage(result.assistant),
	}
}
