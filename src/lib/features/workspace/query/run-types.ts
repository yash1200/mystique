import type { QueryBatchResult, QueryMode, QueryResult } from '#core/contracts'

export type RunState =
	| { status: 'idle' }
	| {
			status: 'running'
			/** 0-based statement index while a batch is in flight. */
			progress?: { index: number; total: number }
	  }
	| {
			status: 'ok' | 'blocked' | 'error'
			message: string
			mode: QueryMode
			elapsedMs: number
			result?: QueryResult | null
			/** Present after a batch run. One entry for a single statement. */
			batch?: QueryBatchResult | null
	  }

export type QueryRunCache = {
	/** Includes `running`, so leaving the tab does not drop an in-flight query. */
	runState: RunState
	resultPage: number
	/** Last text sent, so refresh repeats that statement rather than the whole script. */
	submittedText?: string
	/** Selected result-set chip. Restored when the tab is shown again. */
	activeIndex?: number
}
