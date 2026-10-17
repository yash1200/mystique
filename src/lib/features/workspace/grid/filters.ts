import { isLargeCell, largeCellText } from './large-cell'
import type { FilterOp, FilterRule } from './types'
export const FILTER_OPS: {
	value: FilterOp
	label: string
	needsValue: boolean
}[] = [
	{ value: 'eq', label: '= equals', needsValue: true },
	{ value: 'neq', label: '≠ not equal', needsValue: true },
	{ value: 'gt', label: '> greater', needsValue: true },
	{ value: 'gte', label: '≥ greater or equal', needsValue: true },
	{ value: 'lt', label: '< less', needsValue: true },
	{ value: 'lte', label: '≤ less or equal', needsValue: true },
	{ value: 'contains', label: 'contains', needsValue: true },
	{ value: 'starts', label: 'starts with', needsValue: true },
	{ value: 'ends', label: 'ends with', needsValue: true },
	{ value: 'is_null', label: 'is null', needsValue: false },
	{ value: 'not_null', label: 'is not null', needsValue: false },
	{ value: 'is_empty', label: 'is empty', needsValue: false },
	{ value: 'not_empty', label: 'is not empty', needsValue: false },
]

export const opOptions = FILTER_OPS.map((o) => ({
	value: o.value,
	label: o.label,
}))

export function opNeedsValue(op: FilterOp): boolean {
	return FILTER_OPS.find((o) => o.value === op)?.needsValue ?? true
}

export function newRule(defaultColumn = ''): FilterRule {
	return {
		id: `f-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
		column: defaultColumn,
		op: 'eq',
		value: '',
	}
}

export function looksLikeJsonFilter(code: string): boolean {
	const t = code.trim()
	return t.startsWith('{') && t.endsWith('}')
}

export function parseCodeFilters(code: string): FilterRule[] | null {
	const trimmed = code.trim()
	if (!trimmed) return []
	if (looksLikeJsonFilter(trimmed)) {
		// Handled in the code-filter box only — never a UI operator.
		return []
	}
	const lines = code
		.split(/\n+|(?:\s+AND\s+)/i)
		.map((l) => l.trim())
		.filter(Boolean)
	if (lines.length === 0) return []
	const rules: FilterRule[] = []
	for (const line of lines) {
		const m = line.match(
			/^([A-Za-z_][\w.]*)\s*(=|!=|<>|>=|<=|>|<|contains|starts with|ends with|is null|is not null|is empty|is not empty)\s*(.*)$/i,
		)
		if (!m) return null
		const col = m[1]
		const opRaw = m[2].toLowerCase()
		let val = m[3].trim()
		if (
			(val.startsWith("'") && val.endsWith("'")) ||
			(val.startsWith('"') && val.endsWith('"'))
		) {
			val = val.slice(1, -1)
		}
		let op: FilterOp = 'eq'
		if (opRaw === '=') op = 'eq'
		else if (opRaw === '!=' || opRaw === '<>') op = 'neq'
		else if (opRaw === '>') op = 'gt'
		else if (opRaw === '>=') op = 'gte'
		else if (opRaw === '<') op = 'lt'
		else if (opRaw === '<=') op = 'lte'
		else if (opRaw === 'contains') op = 'contains'
		else if (opRaw === 'starts with') op = 'starts'
		else if (opRaw === 'ends with') op = 'ends'
		else if (opRaw === 'is null') op = 'is_null'
		else if (opRaw === 'is not null') op = 'not_null'
		else if (opRaw === 'is empty') op = 'is_empty'
		else if (opRaw === 'is not empty') op = 'not_empty'
		rules.push({
			id: `f-${rules.length}-${col}`,
			column: col,
			op,
			value: val,
		})
	}
	return rules
}

export function rulesToCode(rules: FilterRule[]): string {
	return rules
		.filter((r) => r.column)
		.map((r) => {
			const col = r.column
			switch (r.op) {
				case 'eq':
					return `${col} = '${r.value}'`
				case 'neq':
					return `${col} != '${r.value}'`
				case 'gt':
					return `${col} > ${r.value}`
				case 'gte':
					return `${col} >= ${r.value}`
				case 'lt':
					return `${col} < ${r.value}`
				case 'lte':
					return `${col} <= ${r.value}`
				case 'contains':
					return `${col} contains '${r.value}'`
				case 'starts':
					return `${col} starts with '${r.value}'`
				case 'ends':
					return `${col} ends with '${r.value}'`
				case 'is_null':
					return `${col} is null`
				case 'not_null':
					return `${col} is not null`
				case 'is_empty':
					return `${col} is empty`
				case 'not_empty':
					return `${col} is not empty`
			}
		})
		.join('\nAND ')
}

export function cellText(value: unknown): string {
	if (isLargeCell(value)) return largeCellText(value)
	if (value === null || value === undefined) return 'NULL'
	if (typeof value === 'boolean') return value ? 'true' : 'false'
	return String(value)
}

export function valuesEqual(a: unknown, b: unknown): boolean {
	if (a === b) return true
	if (a == null && b == null) return true
	return String(a) === String(b)
}

/** Column type tag for a JSON string that is really an int8 or numeric. */
export function numericColumn(dataType: string | null | undefined): boolean {
	const t = (dataType ?? '').trim().toLowerCase()
	if (!t || t.includes('interval')) return false
	const base = t.split('(')[0]?.replace(/\[\]$/, '') ?? ''
	return (
		base.includes('int') ||
		base.includes('serial') ||
		base.includes('numeric') ||
		base.includes('decimal') ||
		base.includes('float') ||
		base.includes('double') ||
		base.includes('real') ||
		base.includes('money') ||
		base === 'number' ||
		base === 'oid' ||
		base === 'year'
	)
}

/**
 * Digit text that must stay a string. `Number` would round it, and a grid
 * `WHERE` would then hit a neighbor row.
 */
export function exactNumberText(text: string): string | number | null {
	const t = text.trim()
	if (!/^[+-]?\d+(\.\d+)?$/.test(t)) return null
	if (t.includes('.') || !Number.isSafeInteger(Number(t))) return t
	return Number(t)
}

export function parseDraft(raw: string, previous: unknown): unknown {
	const t = raw.trim()
	if (t === '' || t.toUpperCase() === 'NULL') return null
	if (typeof previous === 'boolean') {
		const lower = t.toLowerCase()
		if (lower === 'true' || lower === '1' || lower === 'yes') return true
		if (lower === 'false' || lower === '0' || lower === 'no') return false
	}
	const exact = exactNumberText(t)
	if (exact !== null && typeof previous !== 'boolean') {
		if (typeof exact === 'string') return exact
		if (typeof previous === 'number' || typeof previous === 'string') return exact
	}
	return raw
}
