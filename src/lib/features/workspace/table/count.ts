/**
 * The table grid's background total count, as the status bar shows it.
 *
 * The count starts with the page and never holds it up. Under a time budget
 * a slow COUNT is stopped and the engine's estimate shown as `~N`; the user
 * can then ask for the exact count (no budget) or cancel a running one.
 */
import type { TablePreviewCount } from '#core/contracts'

/** Exact COUNT budget for an automatic count. A click on `~N` has none. */
export const COUNT_BUDGET_MS = 3_000

export type TableCountState =
	| { status: 'counting'; cancelling: boolean; exact: boolean }
	| { status: 'exact'; total: number }
	/** Not exact: over budget, skipped (huge table), cancelled, or failed. */
	| {
			status: 'estimated'
			estimate: number | null
			reason: 'over_budget' | 'skipped' | 'cancelled' | 'failed'
	  }

export function countState(result: TablePreviewCount): TableCountState {
	if (result.totalKnown || result.outcome === 'exact') {
		return { status: 'exact', total: result.totalRows }
	}
	return { status: 'estimated', estimate: result.estimatedRows ?? null, reason: result.outcome }
}

const compact = new Intl.NumberFormat('en', {
	notation: 'compact',
	maximumFractionDigits: 1,
})

/** `~1.2M`, `~950`. */
export function approxCount(n: number): string {
	return `~${n < 10_000 ? n.toLocaleString('en') : compact.format(n)}`
}

function rows(n: number): string {
	return n === 1 ? 'row' : 'rows'
}

/** Status-bar text, and the title that explains it. */
export type CountLabel = {
	text: string
	title: string
	/** What a click does: stop the running count, or count every row. */
	action: 'cancel' | 'count' | null
}

export function countLabel(state: TableCountState | undefined): CountLabel | null {
	if (!state) return null
	switch (state.status) {
		case 'counting':
			return {
				text: state.cancelling ? 'Cancelling…' : 'Counting…',
				title: state.exact
					? 'Counting every row. The grid stays usable.'
					: 'Counting rows in the background. The grid stays usable.',
				action: state.cancelling ? null : 'cancel',
			}
		case 'exact':
			return {
				text: `${state.total.toLocaleString('en')} ${rows(state.total)}`,
				title: 'Exact row count',
				action: null,
			}
		case 'estimated': {
			const why =
				state.reason === 'skipped'
					? 'Large table: not counted.'
					: state.reason === 'over_budget'
						? `The count took over ${COUNT_BUDGET_MS / 1000} s and was stopped.`
						: state.reason === 'cancelled'
							? 'Count cancelled.'
							: 'The count failed.'
			if (state.estimate == null) {
				return { text: 'Count rows', title: `${why} Click to count every row.`, action: 'count' }
			}
			return {
				text: `${approxCount(state.estimate)} ${rows(state.estimate)}`,
				title: `Estimated by the engine. ${why} Click to count every row.`,
				action: 'count',
			}
		}
	}
}
