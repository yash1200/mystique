/**
 * Where a result column came from: `schema.table.column` for the header
 * tooltip, read from the statement text and the catalog.
 *
 * Only the top-level `SELECT … FROM` is read. Plain column references
 * (`c`, `t.c`, `c AS x`), `*` and `t.*` map to a catalog column; expressions,
 * CTE and subquery columns stay unknown. An unqualified name that two FROM
 * tables share is unknown too.
 *
 * TODO(E-12): prefer the engine's result-column provenance once the result
 * DTO carries it (it is being added for large-value locators); this text
 * reading is the fallback.
 *
 * Self-contained, no module-level collections: it is reached through the
 * `#engines` barrel and must not pull into the startup chunk.
 */
import type { CatalogColumnRow } from '#core/contracts'

export type ResultColumnSource = {
	schema: string | null
	table: string
	column: string
	dataType: string | null
}

type Ref = { schema: string | null; table: string; alias: string | null }

/** A bare, `"double"` or `` `back` `` quoted name. */
const NAME = '"(?:[^"]|"")*"|`[^`]*`|[A-Za-z_][\\w$]*'

/** Built on use: a module-level `new RegExp` would count as a side effect. */
function patterns() {
	return {
		simple: new RegExp(`^(?:(${NAME})\\s*\\.\\s*)?(${NAME})(?:\\s+(?:as\\s+)?(${NAME}))?$`, 'i'),
		qualifiedStar: new RegExp(`^(${NAME})\\s*\\.\\s*\\*$`, 'i'),
		word: new RegExp(`(${NAME})`, 'y'),
		tableRef: new RegExp(
			`(${NAME})(?:\\s*\\.\\s*(${NAME}))?(?:\\s+(?:as\\s+)?(${NAME}))?`,
			'iy',
		),
	}
}

type Patterns = ReturnType<typeof patterns>

function stopsFrom(word: string): boolean {
	return [
		'where',
		'group',
		'order',
		'limit',
		'union',
		'except',
		'intersect',
		'having',
		'window',
		'offset',
		'returning',
		'for',
		'fetch',
		'into',
	].includes(word)
}

function notAlias(word: string): boolean {
	return (
		stopsFrom(word) ||
		[
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
			'straight_join',
			'tablesample',
		].includes(word)
	)
}

function unquote(name: string): string {
	if (name.startsWith('"') && name.endsWith('"')) return name.slice(1, -1).replace(/""/g, '"')
	if (name.startsWith('`') && name.endsWith('`')) return name.slice(1, -1)
	return name
}

/** Blank strings, comments and quoted names' insides, keeping indexes. */
function mask(sql: string, mysql: boolean): string {
	let out = ''
	let i = 0
	const blank = (to: number) => {
		for (; i < to && i < sql.length; i++) out += sql[i] === '\n' ? '\n' : ' '
	}
	while (i < sql.length) {
		const ch = sql[i]
		if (sql.startsWith('--', i) || (mysql && ch === '#')) {
			const end = sql.indexOf('\n', i)
			blank(end < 0 ? sql.length : end)
			continue
		}
		if (sql.startsWith('/*', i)) {
			const end = sql.indexOf('*/', i + 2)
			blank(end < 0 ? sql.length : end + 2)
			continue
		}
		if (ch === "'" || ch === '"' || ch === '`') {
			let j = i + 1
			while (j < sql.length) {
				if (ch === "'" && mysql && sql[j] === '\\') {
					j += 2
					continue
				}
				if (sql[j] === ch) {
					if (sql[j + 1] === ch) {
						j += 2
						continue
					}
					break
				}
				j += 1
			}
			// Keep the quotes so a quoted name still reads as one token.
			out += ch
			i += 1
			blank(j)
			if (i < sql.length) {
				out += sql[i]
				i += 1
			}
			continue
		}
		out += ch
		i += 1
	}
	return out
}

type Word = { word: string; start: number; end: number; depth: number }

/** Bare words of the masked text, with their paren depth. */
function words(masked: string): Word[] {
	const found: Word[] = []
	let depth = 0
	const re = /[()]|[A-Za-z_][\w$]*/g
	for (let m = re.exec(masked); m; m = re.exec(masked)) {
		if (m[0] === '(') depth += 1
		else if (m[0] === ')') depth = Math.max(0, depth - 1)
		else found.push({ word: m[0].toLowerCase(), start: m.index, end: m.index + m[0].length, depth })
	}
	return found
}

/** `FROM a x, b JOIN c AS y ON …` at depth 0, from `from` to the clause after it. */
function fromRefs(
	sql: string,
	masked: string,
	fromEnd: number,
	stop: number,
	re: Patterns,
): Ref[] | null {
	const refs: Ref[] = []
	let depth = 0
	let expectTable = true
	let i = fromEnd
	while (i < stop) {
		const ch = masked[i]
		if (ch === '(') {
			depth += 1
			if (expectTable && depth === 1) return null // a derived table
			i += 1
			continue
		}
		if (ch === ')') {
			depth = Math.max(0, depth - 1)
			i += 1
			continue
		}
		if (depth > 0) {
			i += 1
			continue
		}
		if (ch === ',') {
			expectTable = true
			i += 1
			continue
		}
		re.word.lastIndex = i
		const m = /\s/.test(ch ?? '') ? null : re.word.exec(sql)
		if (!m?.[1] || m.index + m[1].length > stop) {
			i += 1
			continue
		}
		const word = m[1].toLowerCase()
		if (word === 'join') {
			expectTable = true
			i += m[1].length
			continue
		}
		if (!expectTable || notAlias(word)) {
			i += m[1].length
			continue
		}
		// table, schema.table, then an optional alias
		re.tableRef.lastIndex = i
		const ref = re.tableRef.exec(sql)
		if (!ref?.[1]) return null
		const first = unquote(ref[1])
		const second = ref[2] ? unquote(ref[2]) : null
		let alias = ref[3] ? unquote(ref[3]) : null
		let length = ref[0].length
		if (alias && ref[3] && notAlias(ref[3].toLowerCase())) {
			length -= ref[0].length - ref[0].toLowerCase().lastIndexOf(ref[3].toLowerCase())
			alias = null
		}
		refs.push({ schema: second ? first : null, table: second ?? first, alias })
		expectTable = false
		i += Math.max(1, length)
	}
	return refs
}

function tableRows(rows: readonly CatalogColumnRow[], ref: Ref): CatalogColumnRow[] {
	const table = ref.table.toLowerCase()
	const schema = ref.schema?.toLowerCase() ?? null
	const hits = rows.filter(
		(r) =>
			r.table.toLowerCase() === table &&
			(schema == null || (r.schema ?? '').toLowerCase() === schema),
	)
	const schemas = [...new Set(hits.map((r) => r.schema ?? ''))]
	if (schemas.length <= 1) return hits
	// The same table name in several schemas: `public` is the search path default.
	const preferred = schemas.find((s) => s.toLowerCase() === 'public')
	return preferred == null ? [] : hits.filter((r) => (r.schema ?? '') === preferred)
}

function source(row: CatalogColumnRow): ResultColumnSource {
	return {
		schema: row.schema ?? null,
		table: row.table,
		column: row.column,
		dataType: row.dataType || null,
	}
}

type Slot = { source: ResultColumnSource | null; name: string | null }

/** One source (or null) per result column, by position, checked by name. */
export function resultColumnSources(
	sql: string,
	columnNames: readonly string[],
	rows: readonly CatalogColumnRow[],
	mysql = false,
): (ResultColumnSource | null)[] {
	const none = columnNames.map(() => null)
	if (!sql.trim() || rows.length === 0 || columnNames.length === 0) return none
	const masked = mask(sql, mysql)
	const all = words(masked)
	const select = all.find((w) => w.depth === 0 && w.word === 'select')
	if (!select) return none
	const from = all.find((w) => w.depth === 0 && w.start > select.end && w.word === 'from')
	if (!from) return none
	const stop =
		all.find((w) => w.depth === 0 && w.start > from.end && stopsFrom(w.word))?.start ?? sql.length
	const re = patterns()
	const refs = fromRefs(sql, masked, from.end, stop, re)
	if (!refs || refs.length === 0) return none

	const resolve = (qualifier: string): Ref | null => {
		const q = qualifier.toLowerCase()
		return (
			refs.find((r) => r.alias?.toLowerCase() === q) ??
			refs.find((r) => !r.alias && r.table.toLowerCase() === q) ??
			null
		)
	}

	// Split the select list at depth-0 commas.
	const items: string[] = []
	let depth = 0
	let begin = select.end
	for (let i = select.end; i < from.start; i++) {
		const ch = masked[i]
		if (ch === '(') depth += 1
		else if (ch === ')') depth -= 1
		else if (ch === ',' && depth === 0) {
			items.push(sql.slice(begin, i))
			begin = i + 1
		}
	}
	items.push(sql.slice(begin, from.start))
	if (items[0]) items[0] = items[0].replace(/^\s*(?:distinct|all)\b/i, '')

	const slots: Slot[] = []
	for (const raw of items) {
		const item = raw.trim()
		if (item === '*') {
			for (const ref of refs) {
				const found = tableRows(rows, ref)
				if (found.length === 0) return none
				for (const row of found) slots.push({ source: source(row), name: row.column })
			}
			continue
		}
		const star = re.qualifiedStar.exec(item)
		if (star?.[1]) {
			const ref = resolve(unquote(star[1]))
			const found = ref ? tableRows(rows, ref) : []
			if (found.length === 0) return none
			for (const row of found) slots.push({ source: source(row), name: row.column })
			continue
		}
		const simple = re.simple.exec(item)
		if (!simple?.[2] || (simple[3] && notAlias(simple[3].toLowerCase()))) {
			slots.push({ source: null, name: null })
			continue
		}
		const column = unquote(simple[2])
		const name = simple[3] ? unquote(simple[3]) : column
		const candidates = simple[1] ? [resolve(unquote(simple[1]))] : refs
		const matches: CatalogColumnRow[] = []
		for (const ref of candidates) {
			if (!ref) continue
			const hit = tableRows(rows, ref).find((r) => r.column.toLowerCase() === column.toLowerCase())
			if (hit) matches.push(hit)
		}
		const only = matches.length === 1 ? matches[0] : undefined
		slots.push({ source: only ? source(only) : null, name })
	}

	const sameName = (a: string | null, b: string) => !!a && a.toLowerCase() === b.toLowerCase()
	if (slots.length === columnNames.length) {
		return columnNames.map((name, i) => {
			const slot = slots[i]
			return slot?.source && sameName(slot.name, name) ? slot.source : null
		})
	}
	// Counts disagree (an unknown table under `*`): match by unique name.
	return columnNames.map((name) => {
		const hits = slots.filter((slot) => sameName(slot.name, name))
		return hits.length === 1 ? (hits[0]?.source ?? null) : null
	})
}

/** Header tooltip: `schema.table.column` and the type, when known. */
export function resultColumnTitle(
	name: string,
	dataType: string | null | undefined,
	src: ResultColumnSource | null | undefined,
): string {
	const lines: string[] = []
	if (src) {
		lines.push([src.schema, src.table, src.column].filter(Boolean).join('.'))
		if (src.column !== name) lines.push(`as ${name}`)
	} else {
		lines.push(name)
	}
	const type = dataType || src?.dataType
	if (type) lines.push(`Type: ${type}`)
	return lines.join('\n')
}
