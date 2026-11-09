import type {
	AgentProviderInfo as AgentProviderInfoRow,
	AiKeyStatus as AiKeyStatusRow,
	AiSettingsView as AiSettingsViewRow,
} from '#core/contracts'

export type {
	AcpEnvInput,
	AcpEnvVar,
	AcpProbe,
	AcpSettingsView,
	AgentChat,
	AgentErrorKind,
	AgentEvent,
	AgentFixRequest,
	AgentMessage,
	AgentModelInfo,
	AgentTableRef,
	AgentTokens,
	SaveAcpSettings,
	SendAgentResult,
} from '#core/contracts'

export type AgentProviderId =
	| 'openai'
	| 'anthropic'
	| 'google'
	| 'groq'
	| 'together'
	| 'mistral'
	| 'cohere'
	| 'huggingface'
	| 'moonshot'
	| 'deepseek'
	| 'openrouter'
	| 'xai'
	| 'ollama'

/** `AgentProviderInfo` (generated) with `id` narrowed to the known providers. */
export type AgentProviderInfo = Omit<AgentProviderInfoRow, 'id'> & { id: AgentProviderId }

/** `AiKeyStatus` (generated) with `provider` narrowed. */
export type AiKeyStatus = Omit<AiKeyStatusRow, 'provider'> & { provider: AgentProviderId }

export type AiSettingsView = Omit<AiSettingsViewRow, 'keys' | 'providers'> & {
	keys: AiKeyStatus[]
	providers: AgentProviderInfo[]
}
