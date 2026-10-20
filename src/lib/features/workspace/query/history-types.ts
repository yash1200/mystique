export type QueryHistorySource = 'ui' | 'agent' | 'mcp'

export type QueryHistoryEntry = {
	id: string
	sql: string
	ranAt: string
	durationMs: number
	status: 'success' | 'error'
	rowCount?: number
	error?: string
	connectionId?: string
	connectionName?: string
	engine?: string
	language?: string
	database?: string
	affectedRows?: number
	source: QueryHistorySource
}

export interface QueryHistoryItem {
	id: string
	connectionId?: string | null
	engine?: string | null
	body: string
	language?: string | null
	success: boolean
	rowCount?: number | null
	durationMs?: number | null
	errorMessage?: string | null
	ranAt: string
	database?: string | null
	affectedRows?: number | null
	source?: QueryHistorySource | null
}

export interface AppendHistoryInput {
	connectionId?: string | null
	engine?: string | null
	body: string
	language?: string | null
	success: boolean
	rowCount?: number | null
	durationMs?: number | null
	errorMessage?: string | null
	database?: string | null
	affectedRows?: number | null
	source?: QueryHistorySource | null
}
