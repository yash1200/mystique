export interface McpStatus {
	enabled: boolean
	running: boolean
	port?: number | null
	url?: string | null
	token?: string | null
}
