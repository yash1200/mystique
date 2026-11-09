/**
 * Filter bar and sort state of one table tab: UI rules or a code filter,
 * whether they are applied, the single-column sort, column-name completion,
 * and distinct values for the first rule's column.
 *
 * The tab's saved preview (filters, code filter, sort) seeds this state on
 * open and on an FK jump; `seedSignature` tells when a new seed arrived.
 */
import type {
	TableMeta,
	TablePreviewFilter,
	TablePreviewSortDir,
} from '#core/contracts'
import { distinctColumnValues } from '../api'
import {
	columnCompletions,
	distinctCacheKey,
	readDistinctCache,
	DISTINCT_TTL_MS,
	type DistinctCacheEntry,
} from './column-complete'
import {
	looksLikeJsonFilter,
	newRule,
	opNeedsValue,
	parseCodeFilters,
	rulesToCode,
} from './filters'
import type { FilterMode, FilterOp, FilterRule } from './types'

export type PreviewSeed = {
	filters: TablePreviewFilter[] | null
	codeFilter: string | null
	sort: { column: string; dir: TablePreviewSortDir } | null
}

/** What a reload sends for the current filter bar and sort. */
export type ReloadOptions = {
	offset: number
	filters: TablePreviewFilter[]
	codeFilter?: string
	sortColumn?: string
	sortDir?: TablePreviewSortDir
}

export function seedSignature(seed: PreviewSeed): string {
	const filters = (seed.filters ?? []).map((f) => `${f.column}:${f.op}:${f.value}`).join('|')
	const code = seed.codeFilter?.trim() ?? ''
	const sort = seed.sort?.column && seed.sort.dir ? `${seed.sort.column}:${seed.sort.dir}` : ''
	return `${code}\n${filters}\n${sort}`
}

export class TableFilters {
	mode = $state<FilterMode>('ui')
	rules = $state<FilterRule[]>([])
	code = $state('')
	/** Rules or code are applied to the page. */
	active = $state(false)
	/** Single-column preview sort. Switching columns replaces the previous order. */
	sortColumn = $state<string | null>(null)
	sortDir = $state<TablePreviewSortDir | null>(null)
	completions = $state<string[]>([])
	distinctValues = $state<string[]>([])
	/** Signature of the seed last applied. */
	appliedSeed = $state<string | null>(null)
	#completionStart = 0
	#distinctCache = new Map<string, DistinctCacheEntry>()
	#distinctTimer: ReturnType<typeof setTimeout> | undefined
	readonly #table: () => TableMeta
	readonly #sessionId: () => string | null
	readonly #supportsDistinct: () => boolean

	constructor(opts: {
		table: () => TableMeta
		sessionId: () => string | null
		supportsDistinct: () => boolean
	}) {
		this.#table = opts.table
		this.#sessionId = opts.sessionId
		this.#supportsDistinct = opts.supportsDistinct
	}

	columnOptions = $derived(this.#columnNames().map((name) => ({ value: name, label: name })))

	hasActive = $derived(
		this.active &&
			(this.mode === 'code' ? this.code.trim().length > 0 : this.activeRules().length > 0),
	)

	#columnNames(): string[] {
		return this.#table().columns.map((c) => c.name)
	}

	makeRule(): FilterRule {
		return newRule(this.#table().columns[0]?.name ?? '')
	}

	activeRules(): FilterRule[] {
		if (!this.active) return []
		if (this.mode === 'code') return parseCodeFilters(this.code) ?? []
		return this.rules.filter((r) => {
			if (!r.column) return false
			if (!opNeedsValue(r.op)) return true
			return r.value.trim() !== ''
		})
	}

	reloadOptions(offset: number): ReloadOptions {
		const opts: ReloadOptions = {
			offset,
			filters:
				!this.active || this.mode === 'code'
					? []
					: this.activeRules().map((r) => ({ column: r.column, op: r.op, value: r.value })),
		}
		if (this.active && this.mode === 'code' && this.code.trim()) {
			opts.codeFilter = this.code.trim()
		}
		if (this.sortColumn && this.sortDir) {
			opts.sortColumn = this.sortColumn
			opts.sortDir = this.sortDir
		}
		return opts
	}

	/** Ascending, then descending, then off. Another column starts ascending. */
	cycleSort(column: string) {
		if (this.sortColumn === column) {
			if (this.sortDir === 'asc') {
				this.sortDir = 'desc'
			} else {
				this.sortColumn = null
				this.sortDir = null
			}
		} else {
			this.sortColumn = column
			this.sortDir = 'asc'
		}
	}

	setMode(mode: FilterMode) {
		if (mode === this.mode) return
		if (mode === 'code') {
			this.code = rulesToCode(this.rules)
		} else if (looksLikeJsonFilter(this.code)) {
			if (this.rules.length === 0) this.rules = [this.makeRule()]
		} else {
			const parsed = parseCodeFilters(this.code)
			if (parsed && parsed.length > 0) this.rules = parsed
			else if (this.rules.length === 0) this.rules = [this.makeRule()]
		}
		this.mode = mode
	}

	clear() {
		this.active = false
		this.rules = [this.makeRule()]
		this.code = ''
	}

	addRule() {
		this.rules = [...this.rules, this.makeRule()]
	}

	removeRule(id: string) {
		this.rules = this.rules.filter((r) => r.id !== id)
		if (this.rules.length === 0) this.rules = [this.makeRule()]
	}

	updateRule(id: string, patch: Partial<FilterRule>) {
		this.rules = this.rules.map((r) => (r.id === id ? { ...r, ...patch } : r))
	}

	updateFirst(patch: Partial<FilterRule>) {
		const first = this.rules[0]
		if (!first) this.rules = [{ ...this.makeRule(), ...patch }]
		else this.updateRule(first.id, patch)
		if (patch.column) this.#scheduleDistinct(patch.column)
	}

	#scheduleDistinct(column: string) {
		if (this.#distinctTimer) clearTimeout(this.#distinctTimer)
		const sid = this.#sessionId()
		if (!this.#supportsDistinct() || !sid || !column) {
			this.distinctValues = []
			return
		}
		const table = this.#table()
		const key = distinctCacheKey(sid, table.id, column)
		const cached = readDistinctCache(this.#distinctCache, key, Date.now(), DISTINCT_TTL_MS)
		if (cached) {
			this.distinctValues = cached
			return
		}
		this.#distinctTimer = setTimeout(() => {
			void distinctColumnValues({ sessionId: sid, table, column })
				.then((values) => {
					this.#distinctCache.set(key, { at: Date.now(), values })
					this.distinctValues = values
				})
				.catch(() => {
					this.distinctValues = []
				})
		}, 300)
	}

	onCodeInput(value: string, caret: number) {
		const found = columnCompletions(value, caret, this.#columnNames())
		this.#completionStart = found.start
		this.completions = found.items
	}

	pickCompletion(item: string) {
		const start = this.#completionStart
		const typed = this.code.slice(start).match(/^[A-Za-z_][A-Za-z0-9_]*/)?.[0] ?? ''
		this.code = this.code.slice(0, start) + item + this.code.slice(start + typed.length)
		this.completions = []
	}

	/** What the parent would persist if it reloaded from the current filter bar. */
	localSignature(): string {
		const code = this.active && this.mode === 'code' ? this.code.trim() : ''
		const filters =
			!this.active || this.mode === 'code'
				? ''
				: this.activeRules()
						.map((r) => `${r.column}:${r.op}:${r.value}`)
						.join('|')
		const sort = this.sortColumn && this.sortDir ? `${this.sortColumn}:${this.sortDir}` : ''
		return `${code}\n${filters}\n${sort}`
	}

	/** Rebuild the bar and sort from the tab's saved preview. */
	applySeed(seed: PreviewSeed) {
		const codeSeed = seed.codeFilter?.trim() ?? ''
		if (codeSeed) {
			this.mode = 'code'
			this.code = codeSeed
			this.rules = [this.makeRule()]
			this.active = true
		} else if (seed.filters && seed.filters.length > 0) {
			this.rules = seed.filters.map((f) => ({
				id: `f-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
				column: f.column,
				op: (f.op as FilterOp) || 'eq',
				value: f.value ?? '',
			}))
			this.code = rulesToCode(this.rules)
			this.active = true
			this.mode = 'ui'
		} else {
			this.active = false
			this.rules = [this.makeRule()]
			this.code = ''
			this.mode = 'ui'
		}
		if (seed.sort?.column && seed.sort.dir) {
			this.sortColumn = seed.sort.column
			this.sortDir = seed.sort.dir
		} else {
			this.sortColumn = null
			this.sortDir = null
		}
		this.appliedSeed = seedSignature(seed)
	}

	dispose() {
		if (this.#distinctTimer) clearTimeout(this.#distinctTimer)
	}
}
