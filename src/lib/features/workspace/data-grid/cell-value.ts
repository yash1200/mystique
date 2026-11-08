/**
 * One cell value: display text, equality, numeric columns, and parsing an
 * edited draft back into a value. Shared by both grids and the inspector.
 */
import { isLargeCell, largeCellText } from './large-cell'

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
