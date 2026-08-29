export type AgentProviderId =
	| 'openai'
	| 'anthropic'
	| 'google'
	| 'deepseek'
	| 'openrouter'
	| 'xai'
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
	ollamaConfigured: boolean
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
