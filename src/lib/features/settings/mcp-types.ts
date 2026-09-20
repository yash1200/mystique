export interface McpStatus {
	enabled: boolean
	running: boolean
	port?: number | null
	url?: string | null
	token?: string | null
}

export interface McpHarness {
	id: string
	label: string
	mark: string
	hint: string
	configPath: string
	installed: boolean
	stale: boolean
}

export type McpWritePolicy = 'read-only' | 'safe_write' | 'high_risk_write'

export interface McpAllowlistConnection {
	id: string
	name: string
	engine: string
}

export interface McpAccess {
	policy: McpWritePolicy
	allowlist: string[]
	connections: McpAllowlistConnection[]
}
