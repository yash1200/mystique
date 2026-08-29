import type { QueryMode, QueryResult } from '$core/contracts'

export type RunState =
	| { status: 'idle' }
	| { status: 'running' }
	| {
			status: 'ok' | 'blocked' | 'error'
			message: string
			mode: QueryMode
			elapsedMs: number
			result?: QueryResult | null
	  }

export type QueryRunCache = {
	runState: Exclude<RunState, { status: 'running' }>
	resultPage: number
}
