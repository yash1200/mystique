/**
 * CTE output columns and FROM aliases for completion.
 * The catalog schema does not know either name.
 */
import type { CatalogColumnRow } from '#core/contracts'
import type { Completion } from '@codemirror/autocomplete'

import { quoteIdent, type IdentQuote } from './quote'

export type StarRange = {
	from: number
	to: number
	insert: string
}

const CLAUSE = new Set([
	'select',
	'from',
	'where',
	'group',
	'order',
	'having',
	'limit',
	'offset',
	'union',
	'except',
	'intersect',
	'set',
	'update',
	'into',
	'values',
	'returning',
	'window',
])

const NOT_ALIAS = new Set([
	...CLAUSE,
	'on',
	'using',
	'join',
	'inner',
	'left',
	'right',
	'full',
	'cross',
	'natural',
	'outer',
	'lateral',
	'as',
	'and',
	'or',
	'with',
	'only',
	'all',
	'distinct',
	'by',
	'fetch',
	'for',
	'when',
	'then',
	'else',
	'end',
	'case',
	'where',
])

type TableRef = { schema: string | null; table: string }

function isIdentStart(code: number): boolean {
	return (code >= 65 && code <= 90) || (code >= 97 && code <= 122) || code === 95
}

function isIdentByte(code: number): boolean {
	return isIdentStart(code) || (code >= 48 && code <= 57)
}

function skipWs(sql: string, index: number): number {
	let i = index
	while (i < sql.length && (sql[i] === ' ' || sql[i] === '\t' || sql[i] === '\n' || sql[i] === '\r')) {
		i += 1
	}
	return i
}

/** Blank strings and comments so keyword scans keep the caller's indexes. */
function maskSql(sql: string, mysql: boolean): string {
	const chars = [...sql]
	let i = 0
	const blank = (from: number, to: number) => {
		for (let k = from; k < to && k < chars.length; k++) {
			if (chars[k] !== '\n') chars[k] = ' '
		}
	}
	while (i < sql.length) {
		if (sql.startsWith('--', i)) {
			const end = sql.indexOf('\n', i)
			const stop = end < 0 ? sql.length : end
			blank(i, stop)
			i = stop
			continue
		}
		if (mysql && sql[i] === '#') {
			const end = sql.indexOf('\n', i)
			const stop = end < 0 ? sql.length : end
			blank(i, stop)
			i = stop
			continue
		}
		if (sql.startsWith('/*', i)) {
			const end = sql.indexOf('*/', i + 2)
			const stop = end < 0 ? sql.length : end + 2
			blank(i, stop)
			i = stop
			continue
		}
		if (sql[i] === "'") {
			let j = i + 1
			while (j < sql.length) {
				if (mysql && sql[j] === '\\' && j + 1 < sql.length) {
					j += 2
					continue
				}
				if (sql[j] === "'") {
					if (sql[j + 1] === "'") {
						j += 2
						continue
					}
					j += 1
					break
				}
				j += 1
			}
			blank(i, j)
			i = j
			continue
		}
		if (sql[i] === '$') {
			const end = dollarQuoteEnd(sql, i)
			if (end != null) {
				blank(i, end)
				i = end
				continue
			}
		}
		i += 1
	}
	return chars.join('')
}

function dollarQuoteEnd(sql: string, start: number): number | null {
	if (sql[start] !== '$') return null
	let tagEnd = start + 1
	if (sql[tagEnd] === '$') return findCloser(sql, tagEnd + 1, '$$')
	while (
		tagEnd < sql.length &&
		isIdentByte(sql.charCodeAt(tagEnd)) &&
		!(sql.charCodeAt(tagEnd) >= 48 && sql.charCodeAt(tagEnd) <= 57)
	) {
		tagEnd += 1
	}
	if (tagEnd === start + 1 || sql[tagEnd] !== '$') return null
	return findCloser(sql, tagEnd + 1, sql.slice(start, tagEnd + 1))
}

function findCloser(sql: string, index: number, tag: string): number {
	const at = sql.indexOf(tag, index)
	return at < 0 ? sql.length : at + tag.length
}

function readIdent(sql: string, index: number): { word: string; end: number } | null {
	if (!isIdentStart(sql.charCodeAt(index))) return null
	let j = index + 1
	while (j < sql.length && isIdentByte(sql.charCodeAt(j))) j += 1
	return { word: sql.slice(index, j), end: j }
}

function readName(sql: string, index: number): { word: string; end: number } | null {
	const quote = sql[index]
	if (quote === '"' || quote === '`') {
		let j = index + 1
		let word = ''
		while (j < sql.length) {
			if (sql[j] === quote) {
				if (sql[j + 1] === quote) {
					word += quote
					j += 2
					continue
				}
				return { word, end: j + 1 }
			}
			word += sql[j]
			j += 1
		}
		return null
	}
	return readIdent(sql, index)
}

function matchingParen(mask: string, open: number): number {
	let depth = 0
	for (let i = open; i < mask.length; i++) {
		if (mask[i] === '(') depth += 1
		else if (mask[i] === ')') {
			depth -= 1
			if (depth === 0) return i
		}
	}
	return mask.length
}

function splitComma(list: string): string[] {
	const mask = maskSql(list, false)
	const parts: string[] = []
	let start = 0
	let depth = 0
	for (let i = 0; i < mask.length; i++) {
		if (mask[i] === '(') depth += 1
		else if (mask[i] === ')') depth -= 1
		else if (mask[i] === ',' && depth === 0) {
			parts.push(list.slice(start, i))
			start = i + 1
		}
	}
	parts.push(list.slice(start))
	return parts
}

function outputName(item: string): string | null {
	const trimmed = item.trim()
	if (!trimmed || trimmed === '*' || trimmed.endsWith('.*')) return null
	const asMatch = trimmed.match(
		/\bas\s+("([^"]*)"|`([^`]*)`|([A-Za-z_][\w]*))\s*$/i,
	)
	if (asMatch) return asMatch[2] ?? asMatch[3] ?? asMatch[4] ?? null
	const ids = [...trimmed.matchAll(/"([^"]*)"|`([^`]*)`|\b([A-Za-z_][\w]*)\b/g)]
	for (let i = ids.length - 1; i >= 0; i--) {
		const name = ids[i]?.[1] || ids[i]?.[2] || ids[i]?.[3]
		if (!name || NOT_ALIAS.has(name.toLowerCase())) continue
		return name
	}
	return null
}

function selectOutputs(body: string): string[] {
	const mask = maskSql(body, false)
	let i = 0
	let depth = 0
	let listStart = -1
	while (i < mask.length) {
		if (mask[i] === '(') {
			depth += 1
			i += 1
			continue
		}
		if (mask[i] === ')') {
			depth = Math.max(0, depth - 1)
			i += 1
			continue
		}
		if (depth === 0) {
			const word = readIdent(mask, i)
			if (word) {
				if (word.word.toLowerCase() === 'select') {
					listStart = word.end
					break
				}
				i = word.end
				continue
			}
		}
		i += 1
	}
	if (listStart < 0) return []
	let j = listStart
	let d = 0
	let listEnd = mask.length
	while (j < mask.length) {
		if (mask[j] === '(') {
			d += 1
			j += 1
			continue
		}
		if (mask[j] === ')') {
			if (d === 0) {
				listEnd = j
				break
			}
			d -= 1
			j += 1
			continue
		}
		if (d === 0) {
			const word = readIdent(mask, j)
			if (word && word.word.toLowerCase() === 'from') {
				listEnd = j
				break
			}
			if (word) {
				j = word.end
				continue
			}
		}
		j += 1
	}
	const names: string[] = []
	for (const part of splitComma(body.slice(listStart, listEnd))) {
		const name = outputName(part)
		if (name) names.push(name)
	}
	return names
}

function collectCtes(sql: string, mask: string): Map<string, { name: string; columns: string[] }> {
	const found = new Map<string, { name: string; columns: string[] }>()
	let i = 0
	let depth = 0
	while (i < mask.length) {
		if (mask[i] === '(') {
			depth += 1
			i += 1
			continue
		}
		if (mask[i] === ')') {
			depth = Math.max(0, depth - 1)
			i += 1
			continue
		}
		if (mask[i] === '"' || mask[i] === '`') {
			const name = readName(sql, i)
			i = name ? name.end : i + 1
			continue
		}
		const word = depth === 0 ? readIdent(mask, i) : null
		if (word && word.word.toLowerCase() === 'with') {
			i = parseCteList(sql, mask, word.end, found)
			continue
		}
		i = word ? word.end : i + 1
	}
	return found
}

function parseCteList(
	sql: string,
	mask: string,
	index: number,
	found: Map<string, { name: string; columns: string[] }>,
): number {
	let i = index
	while (i < mask.length) {
		i = skipWs(mask, i)
		const name = readIdent(mask, i)
		if (!name || name.word.toLowerCase() === 'recursive') {
			if (name?.word.toLowerCase() === 'recursive') {
				i = name.end
				continue
			}
			break
		}
		i = skipWs(mask, name.end)
		let explicit: string[] | null = null
		if (mask[i] === '(') {
			const close = matchingParen(mask, i)
			const after = skipWs(mask, close + 1)
			const asWord = readIdent(mask, after)
			if (asWord && asWord.word.toLowerCase() === 'as') {
				explicit = []
				for (const part of splitComma(sql.slice(i + 1, close))) {
					const col = outputName(part)
					if (col) explicit.push(col)
				}
				i = after
			}
		}
		const asWord = readIdent(mask, i)
		if (!asWord || asWord.word.toLowerCase() !== 'as') break
		i = skipWs(mask, asWord.end)
		if (mask[i] !== '(') break
		const close = matchingParen(mask, i)
		const columns = explicit ?? selectOutputs(sql.slice(i + 1, close))
		found.set(name.word.toLowerCase(), { name: name.word, columns })
		i = skipWs(mask, close + 1)
		if (mask[i] === ',') {
			i += 1
			continue
		}
		break
	}
	return i
}

function collectAliases(sql: string, mask: string): Map<string, TableRef> {
	const aliases = new Map<string, TableRef>()
	let i = 0
	let depth = 0
	while (i < mask.length) {
		if (mask[i] === '(') {
			depth += 1
			i += 1
			continue
		}
		if (mask[i] === ')') {
			depth = Math.max(0, depth - 1)
			i += 1
			continue
		}
		if (mask[i] === '"' || mask[i] === '`') {
			const name = readName(sql, i)
			i = name ? name.end : i + 1
			continue
		}
		const word = readIdent(mask, i)
		if (!word) {
			i += 1
			continue
		}
		const kw = word.word.toLowerCase()
		if (kw === 'from' || kw === 'join') {
			i = parseTableRef(sql, mask, word.end, aliases)
			continue
		}
		i = word.end
	}
	return aliases
}

function parseTableRef(
	sql: string,
	mask: string,
	index: number,
	aliases: Map<string, TableRef>,
): number {
	let i = skipWs(mask, index)
	let name = readName(sql, i)
	if (name && name.word.toLowerCase() === 'only') {
		i = skipWs(mask, name.end)
		name = readName(sql, i)
	}
	if (!name) return i
	let schema: string | null = null
	let table = name.word
	i = skipWs(mask, name.end)
	if (mask[i] === '.') {
		const second = readName(sql, skipWs(mask, i + 1))
		if (second) {
			schema = table
			table = second.word
			i = skipWs(mask, second.end)
		}
	}
	const maybeAs = readIdent(mask, i)
	if (maybeAs && maybeAs.word.toLowerCase() === 'as') {
		i = skipWs(mask, maybeAs.end)
	}
	const alias = readIdent(mask, i)
	if (alias && !NOT_ALIAS.has(alias.word.toLowerCase())) {
		aliases.set(alias.word.toLowerCase(), { schema, table })
		return alias.end
	}
	return i
}

function columnsFor(rows: CatalogColumnRow[], ref: TableRef): string[] {
	const table = ref.table.toLowerCase()
	const schema = ref.schema?.toLowerCase()
	return rows
		.filter((row) => {
			if (row.table.toLowerCase() !== table) return false
			if (!schema) return true
			return (row.schema ?? '').toLowerCase() === schema
		})
		.map((row) => row.column)
}

function clauseAt(mask: string, pos: number): string | null {
	let depth = 0
	let last: string | null = null
	let i = 0
	const stop = Math.min(pos, mask.length)
	while (i < stop) {
		if (mask[i] === '(') {
			depth += 1
			i += 1
			continue
		}
		if (mask[i] === ')') {
			depth = Math.max(0, depth - 1)
			i += 1
			continue
		}
		if (mask[i] === '"' || mask[i] === '`') {
			const name = readName(mask, i)
			i = name ? name.end : i + 1
			continue
		}
		if (depth === 0) {
			const word = readIdent(mask, i)
			if (word) {
				const kw = word.word.toLowerCase()
				if (CLAUSE.has(kw)) last = kw
				i = word.end
				continue
			}
		}
		i += 1
	}
	return last
}

function qualifierBefore(
	sql: string,
	pos: number,
): { name: string; from: number } | null {
	const slice = sql.slice(0, pos)
	const match = slice.match(/(^|[^\w])([A-Za-z_][\w]*)\s*\.\s*([A-Za-z_][\w]*)?$/)
	if (!match?.[2]) return null
	const partial = match[3] ?? ''
	return { name: match[2], from: pos - partial.length }
}

function columnOption(name: string, quote: IdentQuote): Completion {
	return {
		label: name,
		type: 'property',
		apply: quoteIdent(quote, name),
	}
}

export function scopeCompletions(
	sql: string,
	pos: number,
	rows: CatalogColumnRow[],
	quote: IdentQuote,
	mysql = false,
): { from: number; options: Completion[] } | null {
	const mask = maskSql(sql, mysql)
	const ctes = collectCtes(sql, mask)
	const aliases = collectAliases(sql, mask)
	const qualifier = qualifierBefore(sql, pos)
	if (qualifier) {
		const key = qualifier.name.toLowerCase()
		const cte = ctes.get(key)
		const names = cte
			? cte.columns
			: aliases.has(key)
				? columnsFor(rows, aliases.get(key) as TableRef)
				: null
		if (!names || names.length === 0) return null
		const partial = sql.slice(qualifier.from, pos).toLowerCase()
		return {
			from: qualifier.from,
			options: names
				.filter((name) => name.toLowerCase().startsWith(partial))
				.map((name) => columnOption(name, quote)),
		}
	}
	if (clauseAt(mask, pos) !== 'select' || ctes.size === 0) return null
	const options: Completion[] = []
	for (const cte of ctes.values()) {
		for (const column of cte.columns) {
			options.push({
				label: `${cte.name}.${column}`,
				type: 'property',
				apply: `${quoteIdent(quote, cte.name)}.${quoteIdent(quote, column)}`,
			})
		}
	}
	if (options.length === 0) return null
	const word = sql.slice(0, pos).match(/[A-Za-z_][\w.]*$/)
	return { from: word ? pos - word[0].length : pos, options }
}

function readTableOnly(
	sql: string,
	mask: string,
	index: number,
): { ref: TableRef; end: number } | null {
	let i = skipWs(mask, index)
	let name = readName(sql, i)
	if (name && name.word.toLowerCase() === 'only') {
		i = skipWs(mask, name.end)
		name = readName(sql, i)
	}
	if (!name || NOT_ALIAS.has(name.word.toLowerCase())) return null
	let schema: string | null = null
	let table = name.word
	i = skipWs(mask, name.end)
	if (mask[i] === '.') {
		const second = readName(sql, skipWs(mask, i + 1))
		if (second) {
			schema = table
			table = second.word
			i = skipWs(mask, second.end)
		}
	}
	return { ref: { schema, table }, end: i }
}

export function expandStarRange(
	sql: string,
	pos: number,
	rows: CatalogColumnRow[],
	quote: IdentQuote,
	mysql = false,
): StarRange | null {
	const mask = maskSql(sql, mysql)
	let star = -1
	const begin = Math.min(pos, sql.length)
	for (let i = begin; i >= 0 && begin - i < 16; i--) {
		if (mask[i] === '*') {
			star = i
			break
		}
	}
	if (star < 0) return null
	let from = star
	let qualifier: string | null = null
	const pre = mask.slice(0, star)
	const quoted = pre.match(/([A-Za-z_][\w]*)\s*\.\s*$/)
	if (quoted?.[1] != null && quoted.index != null) {
		qualifier = quoted[1]
		from = quoted.index
	}
	let look = from - 1
	while (look >= 0 && mask[look] === ' ') look -= 1
	if (look >= 0 && mask[look] === '(') return null
	if (clauseAt(mask, from) !== 'select') return null

	const ctes = collectCtes(sql, mask)
	const aliases = collectAliases(sql, mask)
	let names: string[] = []
	if (qualifier) {
		const key = qualifier.toLowerCase()
		const cte = ctes.get(key)
		if (cte) names = cte.columns
		else if (aliases.has(key)) names = columnsFor(rows, aliases.get(key) as TableRef)
		else names = columnsFor(rows, { schema: null, table: qualifier })
	} else {
		const tables = tablesInFrom(sql, mask, star)
		for (const ref of tables) {
			const cte = ctes.get(ref.table.toLowerCase())
			names = names.concat(cte ? cte.columns : columnsFor(rows, ref))
		}
	}
	if (names.length === 0) return null
	return {
		from,
		to: star + 1,
		insert: names.map((name) => quoteIdent(quote, name)).join(', '),
	}
}

function tablesInFrom(sql: string, mask: string, star: number): TableRef[] {
	let i = star + 1
	let depth = 0
	while (i < mask.length) {
		if (mask[i] === '(') {
			depth += 1
			i += 1
			continue
		}
		if (mask[i] === ')') {
			if (depth === 0) break
			depth -= 1
			i += 1
			continue
		}
		if (depth === 0) {
			const word = readIdent(mask, i)
			if (word) {
				const kw = word.word.toLowerCase()
				if (kw === 'from') return readFromList(sql, mask, word.end)
				if (kw === 'where' || kw === 'group' || kw === 'order' || kw === 'limit' || kw === 'union') {
					break
				}
				i = word.end
				continue
			}
		}
		i += 1
	}
	return []
}

function readFromList(sql: string, mask: string, index: number): TableRef[] {
	const refs: TableRef[] = []
	let i = index
	while (i < mask.length) {
		i = skipWs(mask, i)
		if (mask[i] === ',') {
			i += 1
			continue
		}
		const word = readIdent(mask, i)
		const kw = word?.word.toLowerCase() ?? ''
		if (
			kw === 'where' ||
			kw === 'group' ||
			kw === 'order' ||
			kw === 'limit' ||
			kw === 'union' ||
			kw === 'having' ||
			kw === 'window' ||
			kw === 'offset'
		) {
			break
		}
		if (kw === 'join' || kw === 'inner' || kw === 'left' || kw === 'right' || kw === 'full' || kw === 'cross' || kw === 'outer' || kw === 'natural') {
			i = word ? word.end : i + 1
			continue
		}
		const table = readTableOnly(sql, mask, i)
		if (!table) break
		refs.push(table.ref)
		i = table.end
		i = skipWs(mask, i)
		const asWord = readIdent(mask, i)
		if (asWord && asWord.word.toLowerCase() === 'as') {
			i = skipWs(mask, asWord.end)
			const alias = readIdent(mask, i)
			if (alias) i = alias.end
			continue
		}
		if (asWord && !NOT_ALIAS.has(asWord.word.toLowerCase())) {
			i = asWord.end
		}
	}
	return refs
}

export function expandSelectStar(
	sql: string,
	rows: CatalogColumnRow[],
	quote: IdentQuote,
	mysql = false,
): string | null {
	const star = sql.indexOf('*')
	if (star < 0) return null
	const range = expandStarRange(sql, star + 1, rows, quote, mysql)
	if (!range) return null
	return sql.slice(0, range.from) + range.insert + sql.slice(range.to)
}
