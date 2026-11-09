/**
 * Editor-side copy of `mystique-sql` `params.rs`.
 * The server still rejects a count mismatch. Values are never spliced in.
 */
import type { SplitDialect } from '../editor/statement-span'

export type ParamSlot = {
	/** What the prompt shows (`:id`, `?`, `$1`). */
	label: string
}

export type ParamScan = {
	slots: ParamSlot[]
	error?: string
}

export type ParamKind = 'text' | 'number' | 'bool' | 'null'

export type ParamMemory = {
	kind: ParamKind
	raw: string
}

const HISTORY_KEY = 'mystique.paramHistory'
const HISTORY_CAP = 8

type RawKind =
	| { type: 'named'; name: string }
	| { type: 'question' }
	| { type: 'dollar'; number: number }

type RawPlace = { start: number; end: number; kind: RawKind }

export function scanQueryParams(sql: string, dialect: SplitDialect): ParamScan {
	const found = scanPlaces(sql, dialect)
	const slots: Array<ParamSlot | null> = []
	const names: { name: string; index: number }[] = []
	for (const place of found) {
		if (place.kind.type === 'named') {
			const name = place.kind.name
			const existing = names.find((entry) => entry.name === name)
			if (existing) continue
			names.push({ name, index: slots.length })
			slots.push({ label: `:${name}` })
			continue
		}
		if (place.kind.type === 'question') {
			slots.push({ label: '?' })
			continue
		}
		const number = place.kind.number
		const index = Math.max(0, number - 1)
		while (slots.length <= index) slots.push(null)
		const current = slots[index]
		if (current && current.label.startsWith('$')) continue
		if (current) {
			return {
				slots: slots.filter((slot): slot is ParamSlot => slot != null),
				error: `Cannot mix ${current.label} with $${number} in one query.`,
			}
		}
		slots[index] = { label: `$${number}` }
	}
	return {
		slots: slots.map((slot, index) => slot ?? { label: `$${index + 1}` }),
	}
}

/** Untouched text is missing. Null and a typed empty string are values. */
export function encodeParam(
	kind: ParamKind,
	raw: string,
	touched: boolean,
): string | number | boolean | null | undefined {
	if (kind === 'null') return null
	if (kind === 'bool') return raw !== 'false'
	if (kind === 'number') {
		const text = raw.trim()
		if (!text || !Number.isFinite(Number(text))) return undefined
		return Number(text)
	}
	if (!touched) return undefined
	return raw
}

function readParamHistory(): Record<string, ParamMemory[]> {
	if (typeof localStorage === 'undefined') return {}
	try {
		const raw = localStorage.getItem(HISTORY_KEY)
		if (!raw) return {}
		const parsed = JSON.parse(raw) as unknown
		if (!parsed || typeof parsed !== 'object') return {}
		return parsed as Record<string, ParamMemory[]>
	} catch {
		return {}
	}
}

export function paramHistory(label: string): ParamMemory[] {
	const rows = readParamHistory()[label]
	return Array.isArray(rows) ? rows.slice(0, HISTORY_CAP) : []
}

export function rememberParamValue(label: string, value: ParamMemory) {
	if (typeof localStorage === 'undefined') return
	const all = readParamHistory()
	const prev = Array.isArray(all[label]) ? all[label] : []
	const next = [
		value,
		...prev.filter((row) => !(row.kind === value.kind && row.raw === value.raw)),
	].slice(0, HISTORY_CAP)
	all[label] = next
	localStorage.setItem(HISTORY_KEY, JSON.stringify(all))
}

/** An open `(` or `[` on Postgres. A `:` inside `expr[…]` is a slice. */
type Nest = 'paren' | 'subscript' | 'bracket'

function scanPlaces(sql: string, dialect: SplitDialect): RawPlace[] {
	const found: RawPlace[] = []
	const postgres = dialect === 'postgres'
	const nests: Nest[] = []
	let index = 0
	while (index < sql.length) {
		if (sql.startsWith('--', index)) {
			index = skipLine(sql, index + 2)
			continue
		}
		if (dialect === 'mysql' && sql.charCodeAt(index) === 35) {
			index = skipLine(sql, index + 1)
			continue
		}
		if (sql.startsWith('/*', index)) {
			index = skipBlock(sql, index + 2)
			continue
		}
		if (postgres && escapeStringStart(sql, index)) {
			index = skipString(sql, index + 2, 39, true)
			continue
		}
		if (sql.charCodeAt(index) === 39) {
			index = skipString(sql, index + 1, 39, dialect === 'mysql')
			continue
		}
		if (sql.charCodeAt(index) === 34) {
			index = skipString(sql, index + 1, 34, false)
			continue
		}
		if (dialect === 'mysql' && sql.charCodeAt(index) === 96) {
			index = skipString(sql, index + 1, 96, false)
			continue
		}
		if (sql.charCodeAt(index) === 36) {
			const quoted = dollarQuoteEnd(sql, index)
			if (quoted != null) {
				index = quoted
				continue
			}
			const number = dollarNumber(sql, index)
			if (number) {
				found.push({
					start: index,
					end: number.end,
					kind: { type: 'dollar', number: number.value },
				})
				index = number.end
				continue
			}
		}
		if (postgres) {
			const code = sql.charCodeAt(index)
			if (code === 40) nests.push('paren')
			else if (code === 91) nests.push(opensSubscript(sql, index) ? 'subscript' : 'bracket')
			else if (code === 41 || code === 93) nests.pop()
			else if (
				code === 58 &&
				sql.charCodeAt(index + 1) !== 58 &&
				nests[nests.length - 1] === 'subscript'
			) {
				index += 1
				continue
			}
		}
		// Postgres has no `?` marker: `?`, `?|`, `?&` are jsonb operators.
		if (sql.charCodeAt(index) === 63 && !postgres && !questionOperator(sql, index)) {
			found.push({
				start: index,
				end: index + 1,
				kind: { type: 'question' },
			})
			index += 1
			continue
		}
		if (sql.charCodeAt(index) === 58) {
			const next = sql.charCodeAt(index + 1)
			if (next === 58 || next === 61) {
				index += 2
				continue
			}
			const name = namedIdent(sql, index + 1)
			if (name) {
				found.push({
					start: index,
					end: name.end,
					kind: { type: 'named', name: name.value },
				})
				index = name.end
				continue
			}
		}
		index += 1
	}
	return found
}

function skipLine(sql: string, index: number): number {
	while (index < sql.length && sql.charCodeAt(index) !== 10) index += 1
	return index
}

function skipBlock(sql: string, index: number): number {
	while (index + 1 < sql.length) {
		if (sql.charCodeAt(index) === 42 && sql.charCodeAt(index + 1) === 47) {
			return index + 2
		}
		index += 1
	}
	return sql.length
}

/**
 * Postgres `E'…'` (or `e'…'`): backslash escapes apply inside, so `\'` does
 * not close it. The `E` must start a word (`date'…'` is a plain string).
 */
function escapeStringStart(sql: string, index: number): boolean {
	const code = sql.charCodeAt(index)
	if (code !== 69 && code !== 101) return false
	if (sql.charCodeAt(index + 1) !== 39) return false
	return index === 0 || !isWordCode(sql.charCodeAt(index - 1))
}

/** Identifier characters, `$`, and anything non-ASCII (Unicode identifiers). */
function isWordCode(code: number): boolean {
	return isIdentByte(code) || code === 36 || code > 127
}

function skipString(sql: string, index: number, quote: number, backslash: boolean): number {
	while (index < sql.length) {
		if (backslash && sql.charCodeAt(index) === 92 && index + 1 < sql.length) {
			index += 2
			continue
		}
		if (sql.charCodeAt(index) === quote) {
			if (sql.charCodeAt(index + 1) === quote) {
				index += 2
				continue
			}
			return index + 1
		}
		index += 1
	}
	return sql.length
}

function dollarQuoteEnd(sql: string, start: number): number | null {
	if (sql.charCodeAt(start) !== 36) return null
	let tagEnd = start + 1
	if (sql.charCodeAt(tagEnd) === 36) return findCloser(sql, tagEnd + 1, '$$')
	while (
		tagEnd < sql.length &&
		isIdentByte(sql.charCodeAt(tagEnd)) &&
		!isDigit(sql.charCodeAt(tagEnd))
	) {
		tagEnd += 1
	}
	if (tagEnd === start + 1 || sql.charCodeAt(tagEnd) !== 36) return null
	return findCloser(sql, tagEnd + 1, sql.slice(start, tagEnd + 1))
}

function findCloser(sql: string, index: number, tag: string): number {
	const at = sql.indexOf(tag, index)
	return at < 0 ? sql.length : at + tag.length
}

function dollarNumber(sql: string, start: number): { end: number; value: number } | null {
	let index = start + 1
	if (index >= sql.length || !isDigit(sql.charCodeAt(index))) return null
	let number = 0
	while (index < sql.length && isDigit(sql.charCodeAt(index))) {
		number = number * 10 + (sql.charCodeAt(index) - 48)
		index += 1
	}
	if (number === 0) return null
	return { end: index, value: number }
}

/** `[` right after an expression (`arr[`, `f(x)[`, `"col"[`), not `ARRAY[`. */
function opensSubscript(sql: string, index: number): boolean {
	let end = index
	while (end > 0 && /\s/.test(sql.charAt(end - 1))) end -= 1
	if (end === 0) return false
	const prev = sql.charCodeAt(end - 1)
	if (prev === 41 || prev === 93 || prev === 34) return true
	if (!isIdentByte(prev)) return false
	let word = end - 1
	while (word > 0 && isIdentByte(sql.charCodeAt(word - 1))) word -= 1
	return sql.slice(word, end).toLowerCase() !== 'array'
}

function questionOperator(sql: string, index: number): boolean {
	const next = sql.charCodeAt(index + 1)
	return next === 124 || next === 38
}

function namedIdent(sql: string, start: number): { end: number; value: string } | null {
	const first = sql.charCodeAt(start)
	if (!isIdentStart(first)) return null
	let index = start + 1
	while (index < sql.length && isIdentByte(sql.charCodeAt(index))) index += 1
	return { end: index, value: sql.slice(start, index) }
}

function isIdentStart(code: number): boolean {
	return (code >= 65 && code <= 90) || (code >= 97 && code <= 122) || code === 95
}

function isIdentByte(code: number): boolean {
	return isIdentStart(code) || isDigit(code)
}

function isDigit(code: number): boolean {
	return code >= 48 && code <= 57
}
