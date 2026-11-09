import { Channel, invoke } from '#core/ipc'
import type {
	AcpProbe,
	AcpSettingsView,
	AgentChat,
	AgentEvent,
	AgentFixRequest,
	AgentMessage,
	AgentModelInfo,
	AgentTableRef,
	AiSettingsView,
	SaveAcpSettings,
	SendAgentResult,
} from './types'

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

export async function getAiSettings(): Promise<AiSettingsView> {
	return invoke('get_ai_settings')
}

export async function setAiEnabled(enabled: boolean): Promise<AiSettingsView> {
	return invoke('set_ai_enabled', { enabled })
}

export async function setAiOllamaUrl(url: string): Promise<AiSettingsView> {
	return invoke('set_ai_ollama_url', { url })
}

export async function setAiApiKey(
	provider: string,
	key: string,
): Promise<AiSettingsView> {
	return invoke('set_ai_api_key', { provider, key })
}

export async function listAgentModels(
	provider: string,
): Promise<AgentModelInfo[]> {
	return invoke('list_agent_models', { provider })
}

export async function listAgentChats(
	connectionId: string,
): Promise<AgentChat[]> {
	const rows = await invoke<AgentChat[]>('list_agent_chats', {
		connectionId: asId(connectionId),
	})
	return rows.map(normalizeChat)
}

export async function updateAgentChat(input: {
	id: string
	title?: string | null
	provider?: string | null
	model?: string | null
}): Promise<AgentChat> {
	return normalizeChat(
		await invoke('update_agent_chat', {
			input: { ...input, id: asId(input.id) },
		}),
	)
}

export async function deleteAgentChat(id: string): Promise<boolean> {
	return invoke('delete_agent_chat', { id: asId(id) })
}

export async function listAgentMessages(
	chatId: string,
): Promise<AgentMessage[]> {
	const rows = await invoke<AgentMessage[]>('list_agent_messages', {
		chatId: asId(chatId),
	})
	return rows.map(normalizeMessage)
}

/**
 * Start a turn. Resolves with the chat and the saved user message as soon as
 * the run starts; the reply streams to `onEvent` and ends with `done` or
 * `error`. Deltas arrive batched (every 50–100 ms), never once per token.
 */
export async function sendAgentMessage(
	request: {
		connectionId: string
		connectionName: string
		engine: string
		database?: string | null
		chatId?: string | null
		provider: string
		model: string
		text: string
		editorQuery?: string | null
		mentions?: AgentTableRef[]
		fix?: AgentFixRequest
	},
	onEvent: (event: AgentEvent) => void,
): Promise<SendAgentResult> {
	const channel = new Channel<AgentEvent>()
	channel.onmessage = onEvent
	const result = await invoke<SendAgentResult>('send_agent_message', {
		request: {
			...request,
			connectionId: asId(request.connectionId),
			chatId: request.chatId ? asId(request.chatId) : request.chatId,
		},
		onEvent: channel,
	})
	return {
		chat: normalizeChat(result.chat),
		user: normalizeMessage(result.user),
	}
}

/** Stop the chat's live turn. False when nothing was running. */
export async function cancelAgentMessage(chatId: string): Promise<boolean> {
	return invoke('cancel_agent_message', { chatId: asId(chatId) })
}

/** Settings → AI → Agent: built-in or an external (ACP) agent. */
export async function getAcpSettings(): Promise<AcpSettingsView> {
	return invoke('get_acp_settings')
}

/** Secret-looking environment values are sealed in the vault. */
export async function saveAcpSettings(
	input: SaveAcpSettings,
): Promise<AcpSettingsView> {
	return invoke('save_acp_settings', { input })
}

/** Start the saved external agent, run `initialize`, stop it. */
export async function testAcpAgent(): Promise<AcpProbe> {
	return invoke('test_acp_agent')
}
