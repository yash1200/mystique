/**
 * Lightweight FE-side query checks (stub until EngineQueryPolicy BE lands).
 * Fail closed: unknown / unparsed → block.
 */
import {
	defaultCapabilitiesForEngine,
	queryLangFromCaps,
	resolveCapabilities,
	type EngineCapabilities,
} from '$lib/types/engineCaps'

export type QueryMode = 'read' | 'write'

export type FePolicyResult =
	| { ok: true; summary: string }
	| { ok: false; code: string; message: string }

function stripSqlComments(sql: string): string {
	return sql
		.replace(/\/\*[\s\S]*?\*\//g, ' ')
		.replace(/--[^\n]*/g, ' ')
		.trim()
}

function firstSqlKeyword(sql: string): string | null {
	const cleaned = stripSqlComments(sql)
	if (!cleaned) return null
	const m = cleaned.match(/^([a-zA-Z_]+)/)
	return m ? m[1].toLowerCase() : null
}

/** Split a SQL batch into statements (`;`-separated). Multi-statement is allowed. */
export function splitSqlStatements(sql: string): string[] {
	return stripSqlComments(sql)
		.split(';')
		.map((p) => p.trim())
		.filter(Boolean)
}

const SQL_READ = new Set([
	'select',
	'with',
	'show',
	'describe',
	'desc',
	'explain',
	'values',
	'pragma',
	'table',
])
/** Session utilities (search_path, USE, BEGIN, …) — allowed without write mode. */
const SQL_SESSION = new Set([
	'set',
	'reset',
	'use',
	'begin',
	'start',
	'commit',
	'rollback',
	'savepoint',
	'release',
	'discard',
	'deallocate',
	'prepare',
	'execute',
	'listen',
	'unlisten',
	'notify',
])
/** Inserts, updates, deletes, and DDL — allowed unless the connection is read-only. */
const SQL_WRITE = new Set([
	'insert',
	'update',
	'replace',
	'merge',
	'delete',
	'truncate',
	'drop',
	'alter',
	'create',
	'grant',
	'revoke',
])

function validateOneSql(text: string, mode: QueryMode): FePolicyResult {
	const kw = firstSqlKeyword(text)
	if (!kw) {
		return {
			ok: false,
			code: 'unparsed',
			message: 'Could not classify this SQL. Blocked (fail closed).',
		}
	}
	if (SQL_READ.has(kw)) {
		return { ok: true, summary: `Read SQL (${kw.toUpperCase()}) allowed.` }
	}
	if (SQL_SESSION.has(kw)) {
		return { ok: true, summary: `Session SQL (${kw.toUpperCase()}) allowed.` }
	}
	if (SQL_WRITE.has(kw)) {
		if (mode === 'read') {
			return {
				ok: false,
				code: 'read_only',
				message: `Write statement (${kw.toUpperCase()}) blocked. This connection is read-only.`,
			}
		}
		return {
			ok: true,
			summary: `Write SQL (${kw.toUpperCase()}) allowed.`,
		}
	}
	return {
		ok: false,
		code: 'unparsed',
		message: `Statement type “${kw.toUpperCase()}” is not allowed in this mode.`,
	}
}

function validateSql(text: string, mode: QueryMode): FePolicyResult {
	const stmts = splitSqlStatements(text)
	if (stmts.length === 0) {
		return { ok: false, code: 'empty', message: 'Query is empty.' }
	}
	let lastOk: FePolicyResult = { ok: true, summary: 'SQL allowed.' }
	for (const stmt of stmts) {
		const r = validateOneSql(stmt, mode)
		if (!r.ok) return r
		lastOk = r
	}
	if (stmts.length > 1 && lastOk.ok) {
		return {
			ok: true,
			summary: `${stmts.length} statements · last: ${lastOk.summary}`,
		}
	}
	return lastOk
}

function validateMongo(text: string, mode: QueryMode): FePolicyResult {
	const t = text.trim()
	if (!t) {
		return { ok: false, code: 'empty', message: 'Query is empty.' }
	}
	const lower = t.toLowerCase()

	const isWrite =
		/\binsert(one|many)?\b/.test(lower) ||
		/\bupdate(one|many)?\b/.test(lower) ||
		/\breplaceone\b/.test(lower) ||
		/\bbulkwrite\b/.test(lower) ||
		/\bdelete(one|many)?\b/.test(lower) ||
		/\bdrop(database|collection)?\b/.test(lower) ||
		/\$out\b/.test(lower) ||
		/\$merge\b/.test(lower) ||
		/\bremove\b/.test(lower)

	const isRead =
		/\bfind\b/.test(lower) ||
		/\baggregate\b/.test(lower) ||
		/\bcount\b/.test(lower) ||
		/\bestimateddocumentcount\b/.test(lower) ||
		t.startsWith('{') ||
		t.startsWith('[')

	if (isWrite) {
		if (mode === 'read') {
			return {
				ok: false,
				code: 'read_only',
				message: 'Write operation blocked. This connection is read-only.',
			}
		}
		return {
			ok: true,
			summary: 'Write MQL allowed.',
		}
	}

	if (isRead) {
		return { ok: true, summary: 'Read MQL / filter allowed.' }
	}

	return {
		ok: false,
		code: 'unparsed',
		message: 'Could not classify this Mongo query. Blocked (fail closed).',
	}
}

/** Engine-aware FE validation before (future) BE execute. */
export function validateQueryFe(
	engine: string,
	text: string,
	mode: QueryMode,
	caps?: EngineCapabilities | null,
): FePolicyResult {
	if (queryLanguageFor(engine, caps) === 'mql') {
		return validateMongo(text, mode)
	}
	return validateSql(text, mode)
}

export function queryLanguageFor(
	engine: string,
	caps?: EngineCapabilities | null,
): 'sql' | 'mql' {
	return queryLangFromCaps(resolveCapabilities(engine, caps))
}

export function queryLanguageForEngine(engine: string): 'sql' | 'mql' {
	return queryLanguageFor(engine, defaultCapabilitiesForEngine(engine))
}

/** Empty — new query tabs should not show sample SQL/MQL hints. */
export function queryPlaceholder(_engine: string): string {
	return ''
}
