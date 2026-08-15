/**
 * Query formatting registry (frontend).
 *
 * Each engine maps to a formatter. New engines: add a case in
 * `formatterIdForEngine` and/or register a custom formatter.
 * Prefer frontend over BE — formatting is local and needs no DB session.
 */
import {
	defaultCapabilitiesForEngine,
	queryLangFromCaps,
} from '$lib/types/engineCaps'
import { format as formatSql, type SqlLanguage } from 'sql-formatter'

export type FormatQueryResult =
	| { ok: true; text: string }
	| { ok: false; error: string; text: string }

export type QueryFormatter = (text: string) => FormatQueryResult

/** Built-in SQL dialect formatters (sql-formatter). */
const SQL_DIALECT: Record<string, SqlLanguage> = {
	postgres: 'postgresql',
	postgresql: 'postgresql',
	pg: 'postgresql',
	mysql: 'mysql',
	mariadb: 'mysql',
	sqlite: 'sqlite',
	sql: 'sql',
}

function formatWithSqlDialect(dialect: SqlLanguage): QueryFormatter {
	return (text) => {
		const src = text.trim()
		if (!src) {
			return { ok: false, error: 'Nothing to format.', text }
		}
		try {
			const formatted = formatSql(src, {
				language: dialect,
				tabWidth: 2,
				keywordCase: 'upper',
				dataTypeCase: 'upper',
				functionCase: 'upper',
				logicalOperatorNewline: 'before',
				expressionWidth: 80,
				linesBetweenQueries: 1,
			})
			// Preserve a single trailing newline for editor comfort.
			const out = formatted.replace(/\s+$/, '') + '\n'
			return { ok: true, text: out }
		} catch (e) {
			const msg = e instanceof Error ? e.message : String(e)
			return { ok: false, error: msg || 'Could not format SQL.', text }
		}
	}
}

/**
 * Mongo / MQL: pretty-print bare JSON and shell `db.coll.find({ … }, { … }).sort(…)`.
 * Unquoted keys are shell-style, not JSON — normalize before pretty-print.
 */
const formatMql: QueryFormatter = (text) => {
	const src = text.trim()
	if (!src) {
		return { ok: false, error: 'Nothing to format.', text }
	}

	const shell = formatMongoShell(src)
	if (shell) {
		return { ok: true, text: shell.replace(/\s+$/, '') + '\n' }
	}

	const body = src.replace(/;\s*$/, '').trim()
	if (body.startsWith('{') || body.startsWith('[')) {
		const pretty = prettyShellValue(body)
		return { ok: true, text: pretty.replace(/\s+$/, '') + '\n' }
	}

	const embedded = prettyPrintEmbeddedShell(src)
	if (embedded !== src) {
		return { ok: true, text: embedded.replace(/\s+$/, '') + '\n' }
	}

	// Valid input we chose not to rewrite (already tidy) is success.
	return { ok: true, text: src.replace(/\s+$/, '') + '\n' }
}

function formatMongoShell(src: string): string | null {
	const semi = /;\s*$/.test(src)
	const body = src.replace(/;\s*$/, '').trim()
	const call = matchMethodCall(body)
	if (!call) return null

	const argsInner = call.argsInner.trim()
	const argSlices = splitTopLevelArgs(argsInner)
	if (argSlices === null) return null

	const prettyArgs = argSlices.map((a) => prettyShellValue(a.trim()))
	let out = `${call.prefix}.${call.method}(`
	if (prettyArgs.length === 0) {
		out += ')'
	} else if (
		prettyArgs.length === 1 &&
		!prettyArgs[0].includes('\n') &&
		prettyArgs[0].length < 72
	) {
		out += prettyArgs[0] + ')'
	} else {
		out += '\n'
		out += prettyArgs.map((a) => indentBlock(a, '  ')).join(',\n')
		out += '\n)'
	}

	let rest = call.rest.trim()
	while (rest.startsWith('.')) {
		const chain = matchDotCall(rest)
		if (!chain) break
		const inner = chain.argsInner.trim()
		const pretty = inner ? prettyShellValue(inner) : ''
		const oneLine = !pretty || (!pretty.includes('\n') && pretty.length < 56)
		if (oneLine) {
			out += `.${chain.method}(${pretty})`
		} else {
			out += `\n  .${chain.method}(\n${indentBlock(pretty, '    ')}\n  )`
		}
		rest = chain.rest.trim()
	}
	if (rest && rest !== ';') {
		return prettyPrintEmbeddedShell(src)
	}
	return out + (semi ? ';' : '')
}

function matchMethodCall(
	src: string,
): { prefix: string; method: string; argsInner: string; rest: string } | null {
	const m = src.match(
		/^(.*?)(?:\.)(findOne|aggregate|find|countDocuments|distinct)\s*\(/i,
	)
	if (!m) return null
	const open = m[0].length - 1
	const close = findMatching(src, open)
	if (close < 0) return null
	const prefix = m[1].trim()
	if (!prefix) return null
	return {
		prefix,
		method: m[2],
		argsInner: src.slice(open + 1, close),
		rest: src.slice(close + 1),
	}
}

function matchDotCall(
	src: string,
): { method: string; argsInner: string; rest: string } | null {
	const m = src.match(/^\.(\w+)\s*\(/)
	if (!m) return null
	const open = m[0].length - 1
	const close = findMatching(src, open)
	if (close < 0) return null
	return {
		method: m[1],
		argsInner: src.slice(open + 1, close),
		rest: src.slice(close + 1),
	}
}

function splitTopLevelArgs(inner: string): string[] | null {
	const out: string[] = []
	let i = 0
	while (i < inner.length) {
		while (i < inner.length && /\s/.test(inner[i])) i += 1
		if (i >= inner.length) break
		if (inner[i] === ',') {
			i += 1
			continue
		}
		if (inner[i] === '{' || inner[i] === '[') {
			const end = findMatching(inner, i)
			if (end < 0) return null
			out.push(inner.slice(i, end + 1))
			i = end + 1
			continue
		}
		const start = i
		while (i < inner.length && inner[i] !== ',') i += 1
		const tok = inner.slice(start, i).trim()
		if (tok) out.push(tok)
	}
	return out
}

function prettyShellValue(slice: string): string {
	const t = slice.trim()
	if (!t) return t
	if (t.startsWith('{') || t.startsWith('[')) {
		const parsed = parseShellJson(t)
		if (parsed !== undefined) {
			const json = JSON.stringify(parsed, null, 2)
			return unquoteSafeKeys(json)
		}
	}
	return t
}

function indentBlock(block: string, pad: string): string {
	return block
		.split('\n')
		.map((line) => pad + line)
		.join('\n')
}

function prettyPrintEmbeddedShell(src: string): string {
	let out = ''
	let i = 0
	while (i < src.length) {
		const ch = src[i]
		if (ch === '{' || ch === '[') {
			const end = findMatching(src, i)
			if (end > i) {
				const slice = src.slice(i, end + 1)
				const pretty = prettyShellValue(slice)
				if (pretty !== slice) {
					const lineStart = src.lastIndexOf('\n', i) + 1
					const baseIndent = src.slice(lineStart, i).match(/^\s*/)?.[0] ?? ''
					out += pretty
						.split('\n')
						.map((line, idx) => (idx === 0 ? line : baseIndent + line))
						.join('\n')
					i = end + 1
					continue
				}
			}
		}
		out += ch
		i += 1
	}
	return out
}

function parseShellJson(slice: string): unknown | undefined {
	try {
		return JSON.parse(slice) as unknown
	} catch {
		/* shell objects need key/string normalize */
	}
	try {
		return JSON.parse(normalizeShellToJson(slice)) as unknown
	} catch {
		return undefined
	}
}

/** Quote bare keys and convert `'str'` so JSON.parse can read shell objects. */
function normalizeShellToJson(input: string): string {
	let out = ''
	let i = 0
	let inStr: '"' | "'" | null = null
	let esc = false
	while (i < input.length) {
		const c = input[i]
		if (inStr) {
			if (esc) {
				if (inStr === "'" && c === "'") out += "'"
				else if (inStr === "'" && c === '"') out += '\\"'
				else out += c === '"' && inStr === "'" ? '\\"' : c
				esc = false
				i += 1
				continue
			}
			if (c === '\\') {
				esc = true
				i += 1
				continue
			}
			if (c === inStr) {
				out += '"'
				inStr = null
				i += 1
				continue
			}
			out += c === '"' ? '\\"' : c
			i += 1
			continue
		}
		if (c === '"' || c === "'") {
			inStr = c
			out += '"'
			i += 1
			continue
		}
		if (c === '_' || c === '$' || /[A-Za-z]/.test(c)) {
			const start = i
			i += 1
			while (i < input.length && /[A-Za-z0-9_$]/.test(input[i])) i += 1
			const ident = input.slice(start, i)
			let j = i
			while (j < input.length && /\s/.test(input[j])) j += 1
			if (j < input.length && input[j] === ':') {
				out += `"${ident}"`
				out += input.slice(i, j)
				out += ':'
				i = j + 1
				continue
			}
			out += ident
			continue
		}
		if (c === ',') {
			let j = i + 1
			while (j < input.length && /\s/.test(input[j])) j += 1
			if (j < input.length && (input[j] === '}' || input[j] === ']')) {
				i += 1
				continue
			}
		}
		out += c
		i += 1
	}
	return out
}

function unquoteSafeKeys(prettyJson: string): string {
	return prettyJson.replace(/"([A-Za-z_][A-Za-z0-9_$]*)"\s*:/g, '$1:')
}

function findMatching(src: string, openIdx: number): number {
	const open = src[openIdx]
	const close = open === '{' ? '}' : ']'
	let depth = 0
	let inStr: '"' | "'" | null = null
	let escape = false
	for (let i = openIdx; i < src.length; i++) {
		const c = src[i]
		if (inStr) {
			if (escape) {
				escape = false
				continue
			}
			if (c === '\\') {
				escape = true
				continue
			}
			if (c === inStr) inStr = null
			continue
		}
		if (c === '"' || c === "'") {
			inStr = c
			continue
		}
		if (c === open) depth += 1
		else if (c === close) {
			depth -= 1
			if (depth === 0) return i
		}
	}
	return -1
}

const formatters: Record<string, QueryFormatter> = {
	// SQL family
	postgres: formatWithSqlDialect('postgresql'),
	postgresql: formatWithSqlDialect('postgresql'),
	pg: formatWithSqlDialect('postgresql'),
	mysql: formatWithSqlDialect('mysql'),
	mariadb: formatWithSqlDialect('mysql'),
	sqlite: formatWithSqlDialect('sqlite'),
	sql: formatWithSqlDialect('sql'),
	// Document
	mongodb: formatMql,
	mongo: formatMql,
	mql: formatMql,
}

/**
 * Register or override a formatter for an engine id (future plugins / dialects).
 * Call once at app init if needed.
 */
export function registerQueryFormatter(
	engineId: string,
	formatter: QueryFormatter,
): void {
	formatters[engineId.toLowerCase()] = formatter
}

function resolveFormatter(engine: string): QueryFormatter {
	const id = engine.toLowerCase().trim()
	if (formatters[id]) return formatters[id]
	const caps = defaultCapabilitiesForEngine(id)
	if (queryLangFromCaps(caps) === 'mql') return formatMql
	const fromCaps = caps.sqlDialect.trim().toLowerCase()
	if (fromCaps) return formatWithSqlDialect(fromCaps as SqlLanguage)
	const dialect = SQL_DIALECT[id]
	if (dialect) return formatWithSqlDialect(dialect)
	return formatWithSqlDialect('sql')
}

/** Format query text for the given engine. Never throws. */
export function formatQuery(engine: string, text: string): FormatQueryResult {
	try {
		return resolveFormatter(engine)(text)
	} catch (e) {
		const msg = e instanceof Error ? e.message : String(e)
		return { ok: false, error: msg || 'Format failed.', text }
	}
}
