/**
 * Filter bar and sort state of one table tab: UI rules or a code filter,
 * whether they are applied, the single-column sort, column-name completion,
 * and distinct-value completion for the value being typed.
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
import { columnCompletions } from './column-complete'
import {
	looksLikeJsonFilter,
	newRule,
	opNeedsValue,
	parseCodeFilters,
	rulesToCode,
} from './filters'
import type { FilterMode, FilterOp, FilterRule } from './types'
import { codeValueContext, insertCodeValue, type CodeValueContext } from './value-complete'
import { ValueSuggester } from './value-suggest.svelte'

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
	/** Rule whose value field owns the suggestions, or `code` for the code box. */
	valueTarget = $state<string | null>(null)
	readonly values: ValueSuggester
	/** Signature of the seed last applied. */
	appliedSeed = $state<string | null>(null)
	#completionStart = 0
	#codeValue: CodeValueContext | null = null
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
		this.values = new ValueSuggester({
			enabled: () => this.#supportsDistinct(),
			scope: () => {
				const sid = this.#sessionId()
				return sid ? `${sid}\n${this.#table().id}` : null
			},
			fetch: (column, prefix, limit) => {
				const sid = this.#sessionId()
				if (!sid) return Promise.resolve([])
				return distinctColumnValues({ sessionId: sid, table: this.#table(), column, prefix, limit })
			},
		})
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

	/**
	 * Add one rule from a cell ("Filter = value"). An empty first rule is
	 * reused. In code mode the predicate is appended to `col = 'v'` text; a
	 * JSON or unparsed code filter is left alone. Returns why it could not.
	 */
	addCellRule(column: string, op: FilterOp, value: string): string | null {
		const rule: FilterRule = { ...newRule(column), op, value }
		if (this.mode === 'code') {
			const code = this.code.trim()
			const parsed = code ? parseCodeFilters(code) : []
			if (!parsed || looksLikeJsonFilter(code)) {
				return 'Clear the code filter to add a filter from a cell.'
			}
			this.code = rulesToCode([...parsed, rule])
			return null
		}
		const kept = this.active
			? this.rules.filter((r) => r.column && (!opNeedsValue(r.op) || r.value.trim() !== ''))
			: []
		const same = kept.some((r) => r.column === column && r.op === op && r.value === value)
		this.rules = same ? kept : [...kept, rule]
		return null
	}

	updateFirst(patch: Partial<FilterRule>) {
		const first = this.rules[0]
		if (!first) this.rules = [{ ...this.makeRule(), ...patch }]
		else this.updateRule(first.id, patch)
	}

	/** The value field of rule `id` was focused or typed in. */
	onRuleValueInput(id: string) {
		const rule = this.rules.find((r) => r.id === id)
		if (!rule || !rule.column || !opNeedsValue(rule.op)) {
			this.closeValues()
			return
		}
		this.valueTarget = id
		this.values.request(rule.column, rule.value)
	}

	closeValues() {
		this.valueTarget = null
		this.#codeValue = null
		this.values.clear()
	}

	/** Put a suggested value in the field (or the code filter) it was asked for. */
	pickValue(value: string): number | null {
		const target = this.valueTarget
		const ctx = this.#codeValue
		this.closeValues()
		if (target === 'code' && ctx) {
			const next = insertCodeValue(this.code, ctx, value)
			this.code = next.code
			return next.caret
		}
		if (target) this.updateRule(target, { value })
		return null
	}

	onCodeInput(value: string, caret: number) {
		const found = columnCompletions(value, caret, this.#columnNames())
		this.#completionStart = found.start
		this.completions = found.items
		const ctx = found.items.length > 0 ? null : codeValueContext(value, caret, this.#columnNames())
		if (!ctx) {
			if (this.valueTarget === 'code') this.closeValues()
			return
		}
		this.#codeValue = ctx
		this.valueTarget = 'code'
		this.values.request(ctx.column, ctx.prefix)
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
		this.values.dispose()
	}
}
