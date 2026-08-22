/**
 * Lightweight syntax highlighter for agent chat fences.
 * Colors come from design-language `--syntax-*` tokens.
 */
import { inferFenceLang } from './chatMarkdown'
import { formatQuery } from './formatQuery'

export type SyntaxKind =
	| 'keyword'
	| 'string'
	| 'number'
	| 'comment'
	| 'operator'
	| 'punct'
	| 'name'
	| 'property'
	| 'type'
	| 'func'
	| 'atom'
	| 'text'

export type SyntaxToken = { kind: SyntaxKind; text: string }

const SQL_KEYWORDS = new Set(
	`
		select from where join left right inner outer full cross natural
		on and or not in is null like ilike between exists case when then else end
		as with recursive union all distinct group by order having limit offset
		insert into values update set delete create table alter drop index view
		primary key foreign references default cascade check constraint unique
		asc desc using apply over partition rows range unbounded preceding
		following current row cast coalesce nullif returning window filter
		lateral unnest array any some except intersect fetch first next only
		for share skip locked nowait conflict do nothing explain analyze verbose
		if exists replace temp temporary schema database column add rename to
		truncate comment grant revoke begin commit rollback transaction
		true false unknown
	`
		.trim()
		.split(/\s+/)
		.filter(Boolean),
)

const SQL_TYPES = new Set(
	`
		int integer bigint smallint tinyint serial bigserial
		numeric decimal real double float money
		char varchar text citext uuid json jsonb xml
		bool boolean bit bytea blob clob
		date time timestamp timestamptz interval
		int2 int4 int8 float4 float8
	`
		.trim()
		.split(/\s+/)
		.filter(Boolean),
)

const JS_KEYWORDS = new Set(
	`
		const let var function return if else for while do switch case break
		continue new class extends this super import export from default
		try catch finally throw typeof instanceof in of async await yield
		delete void
	`
		.trim()
		.split(/\s+/)
		.filter(Boolean),
)

const JS_ATOMS = new Set([
	'true',
	'false',
	'null',
	'undefined',
	'NaN',
	'Infinity',
])

const HIGHLIGHT_MAX = 20_000
const PRETTY_MAX = 8_000

export function highlightCode(lang: string, source: string): SyntaxToken[] {
	if (source.length > HIGHLIGHT_MAX) {
		return [{ kind: 'text', text: source }]
	}
	const kind = classifyLang(lang, source)
	if (kind === 'sql') return tokenizeSql(source)
	if (kind === 'json') return tokenizeJson(source)
	if (kind === 'js') return tokenizeJs(source)
	return tokenizeGeneric(source)
}

export function classifyLang(
	lang: string,
	source: string,
): 'sql' | 'json' | 'js' | 'plain' {
	const l = inferFenceLang(lang, source)
	if (
		l === 'sql' ||
		l === 'postgresql' ||
		l === 'postgres' ||
		l === 'mysql' ||
		l === 'mariadb' ||
		l === 'sqlite' ||
		l === 'plsql' ||
		l === 'tsql'
	) {
		return 'sql'
	}
	if (l === 'json') return 'json'
	if (
		l === 'javascript' ||
		l === 'js' ||
		l === 'ts' ||
		l === 'typescript' ||
		l === 'mql' ||
		l === 'mongodb' ||
		l === 'mongo'
	) {
		return 'js'
	}
	return 'plain'
}

/** Pretty-print SQL / MQL / JSON. Falls back to the original on failure. */
export function prettifyCode(
	lang: string,
	source: string,
	engine: string,
): string {
	const raw = source.replace(/\s+$/, '')
	if (!raw.trim()) return source
	if (raw.length > PRETTY_MAX) {
		return source.endsWith('\n') ? source : `${source}\n`
	}
	const kind = classifyLang(lang, raw)
	if (kind === 'json') {
		try {
			return JSON.stringify(JSON.parse(raw), null, 2) + '\n'
		} catch {
			return source.endsWith('\n') ? source : source + '\n'
		}
	}
	if (kind === 'sql') {
		const result = formatQuery(engine || 'sql', raw)
		return result.ok ? result.text : raw + '\n'
	}
	if (kind === 'js') {
		const id = /mongo|mql/.test(lang.toLowerCase())
			? 'mongodb'
			: engine.toLowerCase() === 'mongodb' || engine.toLowerCase() === 'mongo'
				? 'mongodb'
				: ''
		if (id) {
			const result = formatQuery(id, raw)
			if (result.ok) return result.text
		}
		return raw + '\n'
	}
	return source.endsWith('\n') ? source : source + '\n'
}

export function tokenClass(kind: SyntaxKind): string {
	switch (kind) {
		case 'keyword':
			return 'text-syntax-keyword'
		case 'string':
			return 'text-syntax-string'
		case 'number':
			return 'text-syntax-number'
		case 'comment':
			return 'text-syntax-comment'
		case 'operator':
			return 'text-syntax-operator'
		case 'punct':
			return 'text-syntax-punct'
		case 'name':
			return 'text-syntax-name'
		case 'property':
			return 'text-syntax-property'
		case 'type':
			return 'text-syntax-type'
		case 'func':
			return 'text-syntax-func'
		case 'atom':
			return 'text-syntax-atom'
		default:
			return 'text-fg'
	}
}

function tokenizeSql(src: string): SyntaxToken[] {
	const out: SyntaxToken[] = []
	let i = 0
	let afterDot = false
	while (i < src.length) {
		const c = src[i]
		if (c === '-' && src[i + 1] === '-') {
			const end = src.indexOf('\n', i)
			const stop = end < 0 ? src.length : end
			out.push({ kind: 'comment', text: src.slice(i, stop) })
			i = stop
			afterDot = false
			continue
		}
		if (c === '/' && src[i + 1] === '*') {
			const end = src.indexOf('*/', i + 2)
			const stop = end < 0 ? src.length : end + 2
			out.push({ kind: 'comment', text: src.slice(i, stop) })
			i = stop
			afterDot = false
			continue
		}
		if (c === "'") {
			const stop = readSqlString(src, i, "'")
			out.push({ kind: 'string', text: src.slice(i, stop) })
			i = stop
			afterDot = false
			continue
		}
		if (c === '"') {
			const stop = readSqlString(src, i, '"')
			out.push({
				kind: afterDot ? 'property' : 'name',
				text: src.slice(i, stop),
			})
			i = stop
			afterDot = false
			continue
		}
		if (c === '`' || c === '[') {
			const close = c === '`' ? '`' : ']'
			const end = src.indexOf(close, i + 1)
			const stop = end < 0 ? src.length : end + 1
			out.push({
				kind: afterDot ? 'property' : 'name',
				text: src.slice(i, stop),
			})
			i = stop
			afterDot = false
			continue
		}
		if (c === '$' && /[A-Za-z_0-9]?/.test(src[i + 1] ?? '')) {
			const tagEnd = readDollarTag(src, i)
			if (tagEnd > i) {
				const close = src.indexOf(src.slice(i, tagEnd), tagEnd)
				const stop = close < 0 ? src.length : close + (tagEnd - i)
				out.push({ kind: 'string', text: src.slice(i, stop) })
				i = stop
				afterDot = false
				continue
			}
		}
		if (isDigit(c) || (c === '.' && isDigit(src[i + 1] ?? ''))) {
			const start = i
			i += 1
			while (i < src.length && /[0-9_.]/.test(src[i])) i += 1
			out.push({ kind: 'number', text: src.slice(start, i) })
			afterDot = false
			continue
		}
		if (/[A-Za-z_]/.test(c)) {
			const start = i
			i += 1
			while (i < src.length && /[A-Za-z0-9_$]/.test(src[i])) i += 1
			const word = src.slice(start, i)
			const lower = word.toLowerCase()
			let j = i
			while (j < src.length && /\s/.test(src[j])) j += 1
			const call = src[j] === '('
			let kind: SyntaxKind = 'name'
			if (lower === 'null' || lower === 'true' || lower === 'false')
				kind = 'atom'
			else if (SQL_KEYWORDS.has(lower)) kind = 'keyword'
			else if (SQL_TYPES.has(lower)) kind = 'type'
			else if (call) kind = 'func'
			else if (afterDot) kind = 'property'
			out.push({ kind, text: word })
			afterDot = false
			continue
		}
		if ('.'.includes(c)) {
			out.push({ kind: 'punct', text: c })
			i += 1
			afterDot = true
			continue
		}
		if ('()[],;'.includes(c)) {
			out.push({ kind: 'punct', text: c })
			i += 1
			afterDot = false
			continue
		}
		if ('=<>!+-*/%|&^~'.includes(c)) {
			const start = i
			i += 1
			while (i < src.length && '=<>!+-*/%|&^~'.includes(src[i])) i += 1
			out.push({ kind: 'operator', text: src.slice(start, i) })
			afterDot = false
			continue
		}
		const start = i
		i += 1
		while (
			i < src.length &&
			!/[A-Za-z0-9_'"`.$[\]()=<>!+\-*/%|&^~;,]/.test(src[i]) &&
			src[i] !== '/' &&
			!(src[i] === '-' && src[i + 1] === '-')
		) {
			i += 1
		}
		out.push({ kind: 'text', text: src.slice(start, i) })
	}
	return mergeTokens(out)
}

function tokenizeJson(src: string): SyntaxToken[] {
	const out: SyntaxToken[] = []
	let i = 0
	while (i < src.length) {
		const c = src[i]
		if (c === '"') {
			const stop = readJsString(src, i)
			const slice = src.slice(i, stop)
			let j = stop
			while (j < src.length && /\s/.test(src[j])) j += 1
			out.push({
				kind: src[j] === ':' ? 'property' : 'string',
				text: slice,
			})
			i = stop
			continue
		}
		if (isDigit(c) || (c === '-' && isDigit(src[i + 1] ?? ''))) {
			const start = i
			i += c === '-' ? 1 : 0
			while (i < src.length && /[0-9.eE+-]/.test(src[i])) i += 1
			out.push({ kind: 'number', text: src.slice(start, i) })
			continue
		}
		if (/[A-Za-z]/.test(c)) {
			const start = i
			i += 1
			while (i < src.length && /[A-Za-z]/.test(src[i])) i += 1
			const word = src.slice(start, i)
			out.push({
				kind:
					word === 'true' || word === 'false' || word === 'null'
						? 'atom'
						: 'text',
				text: word,
			})
			continue
		}
		if ('{}[],:'.includes(c)) {
			out.push({ kind: 'punct', text: c })
			i += 1
			continue
		}
		const start = i
		i += 1
		while (i < src.length && !/["0-9A-Za-z{}\[\],:\-]/.test(src[i])) i += 1
		out.push({ kind: 'text', text: src.slice(start, i) })
	}
	return mergeTokens(out)
}

function tokenizeJs(src: string): SyntaxToken[] {
	const out: SyntaxToken[] = []
	let i = 0
	let afterDot = false
	while (i < src.length) {
		const c = src[i]
		if (c === '/' && src[i + 1] === '/') {
			const end = src.indexOf('\n', i)
			const stop = end < 0 ? src.length : end
			out.push({ kind: 'comment', text: src.slice(i, stop) })
			i = stop
			afterDot = false
			continue
		}
		if (c === '/' && src[i + 1] === '*') {
			const end = src.indexOf('*/', i + 2)
			const stop = end < 0 ? src.length : end + 2
			out.push({ kind: 'comment', text: src.slice(i, stop) })
			i = stop
			afterDot = false
			continue
		}
		if (c === '"' || c === "'" || c === '`') {
			const stop = c === '`' ? readTemplate(src, i) : readJsString(src, i)
			out.push({ kind: 'string', text: src.slice(i, stop) })
			i = stop
			afterDot = false
			continue
		}
		if (isDigit(c)) {
			const start = i
			i += 1
			while (i < src.length && /[0-9_.xXa-fA-Fn]/.test(src[i])) i += 1
			out.push({ kind: 'number', text: src.slice(start, i) })
			afterDot = false
			continue
		}
		if (/[A-Za-z_$]/.test(c)) {
			const start = i
			i += 1
			while (i < src.length && /[A-Za-z0-9_$]/.test(src[i])) i += 1
			const word = src.slice(start, i)
			let j = i
			while (j < src.length && /\s/.test(src[j])) j += 1
			const call = src[j] === '('
			let kind: SyntaxKind = 'name'
			if (JS_ATOMS.has(word)) kind = 'atom'
			else if (JS_KEYWORDS.has(word)) kind = 'keyword'
			else if (call) kind = 'func'
			else if (afterDot) kind = 'property'
			out.push({ kind, text: word })
			afterDot = false
			continue
		}
		if (c === '.') {
			out.push({ kind: 'punct', text: c })
			i += 1
			afterDot = true
			continue
		}
		if ('()[]{},;:'.includes(c)) {
			out.push({ kind: 'punct', text: c })
			i += 1
			afterDot = false
			continue
		}
		if ('=<>!+-*/%|&^~?'.includes(c)) {
			const start = i
			i += 1
			while (i < src.length && '=<>!+-*/%|&^~?'.includes(src[i])) i += 1
			out.push({ kind: 'operator', text: src.slice(start, i) })
			afterDot = false
			continue
		}
		const start = i
		i += 1
		out.push({ kind: 'text', text: src.slice(start, i) })
	}
	return mergeTokens(out)
}

function tokenizeGeneric(src: string): SyntaxToken[] {
	const out: SyntaxToken[] = []
	let i = 0
	while (i < src.length) {
		const c = src[i]
		if (c === '/' && src[i + 1] === '/') {
			const end = src.indexOf('\n', i)
			const stop = end < 0 ? src.length : end
			out.push({ kind: 'comment', text: src.slice(i, stop) })
			i = stop
			continue
		}
		if (c === '#' && (i === 0 || src[i - 1] === '\n')) {
			const end = src.indexOf('\n', i)
			const stop = end < 0 ? src.length : end
			out.push({ kind: 'comment', text: src.slice(i, stop) })
			i = stop
			continue
		}
		if (c === '"' || c === "'") {
			const stop = readJsString(src, i)
			out.push({ kind: 'string', text: src.slice(i, stop) })
			i = stop
			continue
		}
		if (isDigit(c)) {
			const start = i
			i += 1
			while (i < src.length && /[0-9._]/.test(src[i])) i += 1
			out.push({ kind: 'number', text: src.slice(start, i) })
			continue
		}
		const start = i
		i += 1
		while (
			i < src.length &&
			!/["'#0-9]/.test(src[i]) &&
			!(src[i] === '/' && src[i + 1] === '/')
		) {
			i += 1
		}
		out.push({ kind: 'text', text: src.slice(start, i) })
	}
	return mergeTokens(out)
}

function readSqlString(src: string, start: number, quote: "'" | '"'): number {
	let i = start + 1
	while (i < src.length) {
		if (src[i] === quote) {
			if (src[i + 1] === quote) {
				i += 2
				continue
			}
			return i + 1
		}
		i += 1
	}
	return src.length
}

function readJsString(src: string, start: number): number {
	const quote = src[start]
	let i = start + 1
	let esc = false
	while (i < src.length) {
		const c = src[i]
		if (esc) {
			esc = false
			i += 1
			continue
		}
		if (c === '\\') {
			esc = true
			i += 1
			continue
		}
		if (c === quote) return i + 1
		if (c === '\n' && quote !== '`') return i
		i += 1
	}
	return src.length
}

function readTemplate(src: string, start: number): number {
	let i = start + 1
	let esc = false
	while (i < src.length) {
		const c = src[i]
		if (esc) {
			esc = false
			i += 1
			continue
		}
		if (c === '\\') {
			esc = true
			i += 1
			continue
		}
		if (c === '`') return i + 1
		i += 1
	}
	return src.length
}

function readDollarTag(src: string, start: number): number {
	if (src[start] !== '$') return start
	let i = start + 1
	while (i < src.length && /[A-Za-z0-9_]/.test(src[i])) i += 1
	if (src[i] === '$') return i + 1
	return start
}

function isDigit(c: string): boolean {
	return c >= '0' && c <= '9'
}

function mergeTokens(tokens: SyntaxToken[]): SyntaxToken[] {
	const out: SyntaxToken[] = []
	for (const t of tokens) {
		const last = out[out.length - 1]
		if (last && last.kind === t.kind) last.text += t.text
		else out.push({ ...t })
	}
	return out
}
