import type { QueryMode, QueryResult } from '#core/contracts'

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
	/** Includes `running`, so leaving the tab does not drop an in-flight query. */
	runState: RunState
	resultPage: number
	/** Last text sent, so refresh repeats that statement rather than the whole script. */
	submittedText?: string
}
