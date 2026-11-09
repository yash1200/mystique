/**
 * Column edit UX helpers: detect boolean / enum columns and build select options.
 */
import type { ColumnDef } from '#core/contracts'
export type SelectOption = {
	/** Serialized draft value; empty string means SQL NULL. */
	value: string
	label: string
}

const NULL_VALUE = ''

/** MySQL: `enum('a','b')` / `set('x','y')` */
function parseMysqlEnumValues(typeStr: string): string[] | null {
	const m = typeStr.match(/^(enum|set)\s*\((.*)\)\s*$/i)
	if (!m) return null
	const inner = m[2]
	const values: string[] = []
	const re = /'((?:\\'|[^'])*)'|"((?:\\"|[^"])*)"/g
	let match: RegExpExecArray | null
	while ((match = re.exec(inner)) !== null) {
		const raw = match[1] ?? match[2] ?? ''
		values.push(raw.replace(/\\'/g, "'").replace(/\\"/g, '"'))
	}
	return values.length > 0 ? values : null
}

function isBooleanType(typeStr: string): boolean {
	const t = (typeStr || '').toLowerCase().trim()
	if (!t) return false
	if (
		t === 'bool' ||
		t === 'boolean' ||
		t === 'bit' ||
		t === 'bit(1)' ||
		t.includes('tinyint(1)')
	) {
		return true
	}
	// mysql_async Debug format
	if (t.includes('mysql_type_tiny') || t === 'tiny') return true
	if (t.includes('mysql_type_bit')) return true
	return false
}

function isEnumType(typeStr: string): boolean {
	const t = (typeStr || '').toLowerCase()
	if (t.startsWith('enum') || t.startsWith('set')) return true
	if (t.includes('mysql_type_enum') || t.includes('mysql_type_set')) return true
	return false
}

/**
 * Whether this column should use a <select> editor.
 * Prefer explicit enumValues; fall back to type heuristics + sample value.
 */
export function usesSelectEditor(col: ColumnDef, sample?: unknown): boolean {
	if (col.enumValues && col.enumValues.length > 0) return true
	if (isBooleanType(col.type)) return true
	if (isEnumType(col.type) && parseMysqlEnumValues(col.type)) return true
	if (typeof sample === 'boolean') return true
	return false
}

function isBooleanColumn(col: ColumnDef, sample?: unknown): boolean {
	if (isBooleanType(col.type)) return true
	if (typeof sample === 'boolean') return true
	// tinyint(1) often arrives as 0/1 without enumValues
	if (
		isBooleanType(col.type) ||
		(col.type.toLowerCase().includes('tinyint') &&
			(sample === 0 || sample === 1 || sample === '0' || sample === '1'))
	) {
		return true
	}
	return typeof sample === 'boolean'
}

/** Options for a select editor, always including NULL first. */
export function selectOptionsForColumn(
	col: ColumnDef,
	sample?: unknown,
): SelectOption[] {
	const opts: SelectOption[] = [{ value: NULL_VALUE, label: 'NULL' }]

	if (
		isBooleanColumn(col, sample) &&
		!(col.enumValues && col.enumValues.length)
	) {
		opts.push({ value: 'true', label: 'true' })
		opts.push({ value: 'false', label: 'false' })
		return opts
	}

	const fromType = parseMysqlEnumValues(col.type)
	const values = col.enumValues?.length ? col.enumValues : (fromType ?? [])

	// Deduplicate while preserving order
	const seen = new Set<string>()
	for (const v of values) {
		if (seen.has(v)) continue
		seen.add(v)
		opts.push({ value: v, label: v })
	}

	// If we still have no options but sample is a non-null scalar, include it
	if (opts.length === 1 && sample != null && sample !== undefined) {
		const s = String(sample)
		opts.push({ value: s, label: s })
	}

	return opts
}

/** Draft string for the select control from a stored cell value. */
export function valueToSelectDraft(value: unknown): string {
	if (value === null || value === undefined) return NULL_VALUE
	if (typeof value === 'boolean') return value ? 'true' : 'false'
	return String(value)
}

/** Parse select draft back to a typed value. */
export function selectDraftToValue(
	draft: string,
	col: ColumnDef,
	previous: unknown,
): unknown {
	if (draft === NULL_VALUE || draft.toUpperCase() === 'NULL') return null
	if (isBooleanColumn(col, previous)) {
		const lower = draft.toLowerCase()
		if (lower === 'true' || lower === '1' || lower === 'yes' || lower === 't') {
			// Preserve numeric 0/1 style if previous was number
			if (typeof previous === 'number') return 1
			return true
		}
		if (lower === 'false' || lower === '0' || lower === 'no' || lower === 'f') {
			if (typeof previous === 'number') return 0
			return false
		}
	}
	return draft
}

