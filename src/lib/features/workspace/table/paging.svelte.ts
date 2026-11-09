/**
 * Server paging of one table tab: the page, the reload spinner, and the
 * page count. COUNT can be deferred (`totalKnown: false`), so a full page
 * means there may be another one.
 */
import type { TableMeta } from '#core/contracts'

export const PAGE_SIZE = 100

/** Rows the pager counts. With no COUNT, a full page leaves room for one more. */
export function pagerTotal(input: {
	rowCount: number
	totalKnown: boolean | undefined
	loaded: number
	page: number
	pageSize?: number
}): number {
	const size = input.pageSize ?? PAGE_SIZE
	const n = Math.max(input.rowCount, input.loaded)
	if (input.totalKnown === false && input.loaded >= size) {
		return Math.max(n, (input.page + 1) * size + 1)
	}
	// A short page is the last one, so count the pages already passed.
	if (input.totalKnown === false) return input.page * size + input.loaded
	return n
}

export class TablePaging {
	page = $state(0)
	refreshing = $state(false)
	/** Second ⌘R confirms discarding a dirty buffer. */
	refreshArmed = $state(false)
	lastFetchedAt = $state<number | null>(Date.now())
	readonly #table: () => TableMeta
	readonly #loaded: () => number

	constructor(opts: { table: () => TableMeta; loaded: () => number }) {
		this.#table = opts.table
		this.#loaded = opts.loaded
		$effect(() => {
			if (this.page > this.totalPages - 1) this.page = Math.max(0, this.totalPages - 1)
		})
	}

	totalPages = $derived.by(() => {
		const table = this.#table()
		const total = pagerTotal({
			rowCount: table.rowCount,
			totalKnown: table.totalKnown,
			loaded: this.#loaded(),
			page: this.page,
		})
		return Math.max(1, Math.ceil(total / PAGE_SIZE) || 1)
	})

	safePage = $derived(Math.min(this.page, this.totalPages - 1))

	/** Run one reload under the spinner. Errors go to `report`. */
	async run(task: () => Promise<void>, report: (err: unknown) => void) {
		this.refreshing = true
		try {
			await task()
		} catch (err) {
			report(err)
		} finally {
			this.refreshing = false
		}
	}
}
