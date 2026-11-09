/**
 * Loaded ER diagrams, keyed by connectionId then database + table. Loading and
 * the last error are per connection too.
 */
import { ConnectionCache } from '#features/workspace/session'
import type { ErDiagramState } from './types'

export type LiveModel = {
	diagram: ErDiagramState
	title: string
	summary: string
	rev: number
	focusNodeId: string
}

/** Key of one diagram inside its connection's slot. */
export function modelKey(database: string, tableId?: string | null): string {
	return `${database.trim() || '_'}::${(tableId ?? '').trim() || '_'}`
}

export const modelStore = new ConnectionCache<LiveModel>()
