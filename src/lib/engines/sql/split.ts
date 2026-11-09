/**
 * Editor copy of `mystique-sql` `split.rs`. Both run the corpus in
 * `crates/mystique-sql/tests/fixtures/split` (see split-parity.test.ts), so
 * change them together. The dialect rules are listed at the top of split.rs.
 *
 * Offsets are string (UTF-16) indexes. Rust reports byte offsets; the
 * statement text is the same.
 */

export type SqlSplitDialect = 'postgres' | 'mysql' | 'sqlite' | 'generic'

export type StatementSpan = {
	start: number
	end: number
	text: string
}

export function sqlSplitDialectFor(engine: string): SqlSplitDialect {
	const id = engine.trim().toLowerCase()
	if (id === 'postgres' || id === 'postgresql' || id === 'cockroachdb' || id === 'pg') {
		return 'postgres'
	}
	if (id === 'mysql' || id === 'mariadb' || id === 'tidb') return 'mysql'
	if (id === 'sqlite' || id === 'turso') return 'sqlite'
	return 'generic'
}

// Plain arrays: a module-level `new Set(…)` counts as a side effect and would
// pull this file into the startup chunk through the `#engines` barrel.
const ROUTINES = ['procedure', 'function', 'trigger', 'event', 'package']
const UNCOUNTED_ENDS = ['if', 'loop', 'while', 'repeat', 'for']

const QUOTE = 39
const DQUOTE = 34
const BACKTICK = 96
const SEMI = 59
const DOLLAR = 36
const SLASH = 47
const STAR = 42
const DASH = 45
const HASH = 35
const BACKSLASH = 92
const NEWLINE = 10

type Stmt = {
	paren: number
	/** Open `BEGIN` / `CASE` inside a routine body. */
	block: number
	first: 'none' | 'ddl' | 'other'
	/** `PROCEDURE`, `FUNCTION`, `TRIGGER`, `EVENT` or `PACKAGE` seen. */
	routine: boolean
}

const freshStmt = (): Stmt => ({ paren: 0, block: 0, first: 'none', routine: false })

/** Split `sql` into statements with their ranges. Comment-only chunks are dropped. */
export function splitSqlStatements(sql: string, dialect: SqlSplitDialect): StatementSpan[] {
	const out: StatementSpan[] = []
	const mysql = dialect === 'mysql'
	const postgres = dialect === 'postgres'
	const dollarQuotes = dialect === 'postgres' || dialect === 'generic'
	let start = 0
	let delim = ';'
	let stmt = freshStmt()

	const finish = (end: number) => {
		pushStmt(out, sql, start, end, dialect)
		stmt = freshStmt()
	}

	/** `DELIMITER xx` at the start of a line; the index after the line, or null. */
	const delimiterCommand = (i: number, end: number): number | null => {
		if (stmt.block !== 0 || !atLineStart(sql, i)) return null
		const after = sql.charCodeAt(end)
		if (after !== 32 && after !== 9) return null
		let k = end
		while (sql.charCodeAt(k) === 32 || sql.charCodeAt(k) === 9) k += 1
		const tokenStart = k
		while (k < sql.length && !isLineSpace(sql.charCodeAt(k))) k += 1
		if (k === tokenStart) return null
		const token = sql.slice(tokenStart, k)
		while (k < sql.length && sql.charCodeAt(k) !== NEWLINE) k += 1
		finish(i)
		delim = token
		start = k
		return k
	}

	const word = (i: number): number => {
		let end = wordEnd(sql, i)
		const custom = delim !== ';'
		if (custom) {
			// `END$$` with `DELIMITER $$`: the word stops at the delimiter.
			for (let k = i + 1; k < end; k += 1) {
				if (sql.startsWith(delim, k)) {
					end = k
					break
				}
			}
		}
		const c = sql.charCodeAt(i)
		if (isDigit(c) || (i > 0 && sql.charCodeAt(i - 1) === 46)) return end
		const w = keyword(sql, i, end)
		if (postgres && w === 'e' && sql.charCodeAt(end) === QUOTE) {
			return skipQuoted(sql, end + 1, QUOTE, true)
		}
		if (mysql && w === 'delimiter') {
			const next = delimiterCommand(i, end)
			if (next != null) return next
		}
		if (stmt.first === 'none') {
			if (w === 'create' || w === 'alter') {
				stmt.first = 'ddl'
				return end
			}
			stmt.first = 'other'
			if (!custom && w === 'begin') {
				// MariaDB anonymous block: `BEGIN NOT ATOMIC … END`.
				const [not, after] = nextWord(sql, end)
				const [atomic] = nextWord(sql, after)
				if (not === 'not' && atomic === 'atomic') stmt.block = 1
			}
			return end
		}
		if (custom) return end
		if (stmt.block === 0) {
			if (stmt.paren !== 0 || stmt.first !== 'ddl') return end
			if (ROUTINES.includes(w)) stmt.routine = true
			else if (stmt.routine && w === 'begin') stmt.block = 1
			return end
		}
		if (w === 'begin' || w === 'case') {
			stmt.block += 1
			return end
		}
		if (w === 'end') {
			const [next, after] = nextWord(sql, end)
			if (UNCOUNTED_ENDS.includes(next)) return after
			stmt.block -= 1
			if (next === 'case') return after
		}
		return end
	}

	let i = 0
	while (i < sql.length) {
		const custom = delim !== ';'
		if (custom && sql.startsWith(delim, i)) {
			finish(i)
			i += delim.length
			start = i
			continue
		}
		const c = sql.charCodeAt(i)
		if (c === QUOTE || c === DQUOTE) {
			i = skipQuoted(sql, i + 1, c, mysql)
		} else if (c === BACKTICK) {
			i = skipQuoted(sql, i + 1, c, false)
		} else if (c === 91 && dialect === 'sqlite') {
			i = skipUntil(sql, i + 1, 93)
		} else if (c === DASH && lineCommentAt(sql, i, dialect)) {
			i = skipLine(sql, i)
		} else if (c === HASH && mysql) {
			i = skipLine(sql, i)
		} else if (c === SLASH && sql.charCodeAt(i + 1) === STAR) {
			i = skipBlockComment(sql, i, dialect)
		} else if (c === DOLLAR && dollarQuotes) {
			const tagEnd = dollarTagEnd(sql, i)
			i = tagEnd == null ? i + 1 : skipDollar(sql, i, tagEnd)
		} else if (c === 40) {
			stmt.paren += 1
			i += 1
		} else if (c === 41) {
			stmt.paren = Math.max(0, stmt.paren - 1)
			i += 1
		} else if (
			c === SEMI &&
			!custom &&
			stmt.block === 0 &&
			(stmt.paren === 0 || !postgres)
		) {
			finish(i)
			start = i + 1
			i += 1
		} else if (isWordCode(c)) {
			i = word(i)
		} else {
			i += 1
		}
	}
	finish(sql.length)
	return out
}

/** Identifier unit. Every UTF-16 unit of a non-ASCII character counts. */
function isWordCode(c: number): boolean {
	return (
		(c >= 48 && c <= 57) ||
		(c >= 65 && c <= 90) ||
		(c >= 97 && c <= 122) ||
		c === 95 ||
		c === DOLLAR ||
		c >= 0x80
	)
}

function isDigit(c: number): boolean {
	return c >= 48 && c <= 57
}

/** Rust's `u8::is_ascii_whitespace` (no vertical tab). */
function isAsciiSpace(c: number): boolean {
	return c === 32 || c === 9 || c === 10 || c === 12 || c === 13
}

function isLineSpace(c: number): boolean {
	return c === 32 || c === 9 || c === 13 || c === 10
}

function wordEnd(sql: string, i: number): number {
	let j = i
	while (j < sql.length && isWordCode(sql.charCodeAt(j))) j += 1
	return j
}

/** The next word (lowercased) after ASCII whitespace, and the index after it. */
function nextWord(sql: string, from: number): [string, number] {
	let i = from
	while (i < sql.length && isAsciiSpace(sql.charCodeAt(i))) i += 1
	const c = sql.charCodeAt(i)
	const end = i < sql.length && isWordCode(c) && c !== DOLLAR ? wordEnd(sql, i) : i
	return [keyword(sql, i, end), end]
}

/**
 * The word lowercased when it could be a keyword (ASCII, at most 9 letters),
 * else ''. Matches Rust's `eq_ignore_ascii_case`: `toLowerCase` alone would
 * turn the Kelvin sign into `k`.
 */
function keyword(sql: string, from: number, to: number): string {
	if (to - from > 9) return ''
	for (let k = from; k < to; k += 1) {
		if (sql.charCodeAt(k) >= 0x80) return ''
	}
	return sql.slice(from, to).toLowerCase()
}

function atLineStart(sql: string, i: number): boolean {
	let k = i
	while (k > 0 && (sql.charCodeAt(k - 1) === 32 || sql.charCodeAt(k - 1) === 9)) k -= 1
	return k === 0 || sql.charCodeAt(k - 1) === NEWLINE
}

/** `--` starts a comment. MySQL wants whitespace (or the end) after it. */
function lineCommentAt(sql: string, i: number, dialect: SqlSplitDialect): boolean {
	if (sql.charCodeAt(i) !== DASH || sql.charCodeAt(i + 1) !== DASH) return false
	if (dialect !== 'mysql') return true
	return i + 2 >= sql.length || sql.charCodeAt(i + 2) <= 32
}

function skipLine(sql: string, from: number): number {
	const at = sql.indexOf('\n', from)
	return at < 0 ? sql.length : at
}

function skipUntil(sql: string, from: number, close: number): number {
	let i = from
	while (i < sql.length && sql.charCodeAt(i) !== close) i += 1
	return Math.min(i + 1, sql.length)
}

/** From just after the opening quote to just after the closing one. */
function skipQuoted(sql: string, from: number, quote: number, backslash: boolean): number {
	let i = from
	while (i < sql.length) {
		const c = sql.charCodeAt(i)
		if (backslash && c === BACKSLASH) {
			i += 2
			continue
		}
		if (c === quote) {
			if (sql.charCodeAt(i + 1) === quote) {
				i += 2
				continue
			}
			return i + 1
		}
		i += 1
	}
	return sql.length
}

/** `i` is at `/*`. Postgres nests block comments. */
function skipBlockComment(sql: string, from: number, dialect: SqlSplitDialect): number {
	const nests = dialect === 'postgres'
	let depth = 1
	let i = from + 2
	while (i < sql.length) {
		const c = sql.charCodeAt(i)
		if (nests && c === SLASH && sql.charCodeAt(i + 1) === STAR) {
			depth += 1
			i += 2
			continue
		}
		if (c === STAR && sql.charCodeAt(i + 1) === SLASH) {
			depth -= 1
			i += 2
			if (depth === 0) return i
			continue
		}
		i += 1
	}
	return sql.length
}

/** `$` + optional tag + `$`. The tag cannot start with a digit (`$1` is a parameter). */
function dollarTagEnd(sql: string, i: number): number | null {
	let j = i + 1
	if (isDigit(sql.charCodeAt(j))) return null
	while (j < sql.length) {
		const c = sql.charCodeAt(j)
		if (!isWordCode(c) || c === DOLLAR) break
		j += 1
	}
	return sql.charCodeAt(j) === DOLLAR ? j + 1 : null
}

function skipDollar(sql: string, i: number, tagEnd: number): number {
	const tag = sql.slice(i, tagEnd)
	const at = sql.indexOf(tag, tagEnd)
	return at < 0 ? sql.length : at + tag.length
}

/** Rust's `char::is_whitespace`, plus the BOM (same set as split.rs). */
function isSpace(c: number): boolean {
	return (
		(c >= 9 && c <= 13) ||
		c === 32 ||
		c === 0x85 ||
		c === 0xa0 ||
		c === 0x1680 ||
		(c >= 0x2000 && c <= 0x200a) ||
		c === 0x2028 ||
		c === 0x2029 ||
		c === 0x202f ||
		c === 0x205f ||
		c === 0x3000 ||
		c === 0xfeff
	)
}

function pushStmt(
	out: StatementSpan[],
	sql: string,
	start: number,
	end: number,
	dialect: SqlSplitDialect,
): void {
	let from = start
	let to = end
	while (from < to && isSpace(sql.charCodeAt(from))) from += 1
	while (to > from && isSpace(sql.charCodeAt(to - 1))) to -= 1
	if (from >= to) return
	const text = sql.slice(from, to)
	if (!hasCode(text, dialect)) return
	out.push({ start: from, end: to, text })
}

/**
 * Anything other than whitespace and comments. A MySQL versioned comment
 * (`/*!`, `/*M!`, `/*T!`) counts: the server runs it.
 */
function hasCode(text: string, dialect: SqlSplitDialect): boolean {
	const mysql = dialect === 'mysql'
	let i = 0
	while (i < text.length) {
		const c = text.charCodeAt(i)
		if (lineCommentAt(text, i, dialect) || (mysql && c === HASH)) {
			i = skipLine(text, i)
		} else if (c === SLASH && text.charCodeAt(i + 1) === STAR) {
			if (mysql && versionedComment(text, i + 2)) return true
			i = skipBlockComment(text, i, dialect)
		} else if (isSpace(c)) {
			i += 1
		} else {
			return true
		}
	}
	return false
}

function versionedComment(text: string, at: number): boolean {
	if (text.charCodeAt(at) === 33) return true
	const m = text[at]?.toLowerCase()
	return (m === 'm' || m === 't') && text.charCodeAt(at + 1) === 33
}
