import type { QueryHistoryKind, QueryHistorySource } from '#core/contracts'

export type {
	AppendHistoryInput,
	QueryHistoryItem,
	QueryHistoryKind,
	QueryHistorySource,
} from '#core/contracts'

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
	/** A query run, or an app action (grid save, structure change, …). */
	kind: QueryHistoryKind
}

