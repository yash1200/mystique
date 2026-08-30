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
