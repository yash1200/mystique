import { Channel, invoke } from '#core/ipc'
import type {
	AcpAgentSaved,
	AcpProbe,
	AcpSettingsView,
	DetectedAcpAgent,
	AgentChat,
	AgentEvent,
	AgentFixRequest,
	AgentMessage,
	AgentModelInfo,
	AgentTableRef,
	AiSettingsView,
	SaveAcpAgent,
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

/** Settings → AI → Agent: the added external (ACP) agents and the default. */
export async function getAcpSettings(): Promise<AcpSettingsView> {
	return invoke('get_acp_settings')
}

/** Add or change an agent. Secret-looking environment values go to the vault. */
export async function saveAcpAgent(input: SaveAcpAgent): Promise<AcpAgentSaved> {
	return invoke('save_acp_agent', { input })
}

/** Remove an agent and its vault secrets. */
export async function removeAcpAgent(id: string): Promise<AcpSettingsView> {
	return invoke('remove_acp_agent', { id })
}

/** What new chats use: `builtin` or an agent id. */
export async function setDefaultAgent(id: string): Promise<AcpSettingsView> {
	return invoke('set_default_agent', { id })
}

/** Which known ACP agents are on this machine. Installs nothing. */
export async function detectAcpAgents(): Promise<DetectedAcpAgent[]> {
	return invoke('detect_acp_agents')
}

/** Add a detected agent with its launch command as found on this machine. */
export async function addDetectedAcpAgent(catalogId: string): Promise<AcpAgentSaved> {
	return invoke('add_detected_acp_agent', { catalogId })
}

/** Start agent `id`, run `initialize` and a sign-in check, stop it. */
export async function testAcpAgent(id: string): Promise<AcpProbe> {
	return invoke('test_acp_agent', { id })
}
