/**
 * Statements in a Mongo shell buffer, for run-at-cursor and the gutter.
 *
 * The server runs one statement at a time, so the editor has to find where
 * each one starts and ends. Shell text is JavaScript-like, not SQL:
 *
 * - `;` ends a statement outside brackets.
 * - A newline outside brackets also ends one, unless the statement clearly
 *   goes on: the line ends with an operator or `,` / `.` / `:`, or the next
 *   code starts with `.` (a chained `.sort()` on its own line), a closing
 *   bracket, `,` or an operator. This is how JavaScript's automatic
 *   semicolons treat these cases.
 * - `( [ {` nest. Strings (`'…'`, `"…"`, backslash escapes, closed by a
 *   newline when unterminated), template literals with `${ … }`, and
 *   `//` / `/* … *\/` comments are opaque.
 * - Comment-only chunks are not statements. A comment above a statement is
 *   part of it, as in SQL.
 */
import type { StatementSpan } from '../sql/split'

const NEWLINE = 10
const SLASH = 47
const STAR = 42
const BACKSLASH = 92
const BACKTICK = 96

// Strings, not module-level Sets: a `new Set(…)` at the top counts as a side
// effect and would pull this file into the startup chunk through `#engines`.
/** The line ends with one of these: the statement goes on. */
const CONTINUES_AFTER = '.,:=+-*/?&|!<>'
/** The next code starts with one of these: it belongs to this statement. */
const CONTINUES_BEFORE = '.)]},?:=+*&|<>'

function oneOf(chars: string, code: number): boolean {
	return code >= 0 && chars.includes(String.fromCharCode(code))
}

const OPENER: Record<number, string> = { 41: '(', 93: '[', 125: '{' }

type Frame = '(' | '[' | '{' | '`' | '${'

export function splitMongoStatements(src: string): StatementSpan[] {
	const out: StatementSpan[] = []
	const stack: Frame[] = []
	let start = 0
	/** Last code character of the current statement, or -1 before any. */
	let last = -1

	const finish = (end: number) => {
		if (last >= 0) pushTrimmed(out, src, start, end)
		last = -1
	}

	let i = 0
	while (i < src.length) {
		const c = src.charCodeAt(i)
		const top = stack[stack.length - 1]
		if (top === '`') {
			if (c === BACKSLASH) {
				i += 2
			} else if (c === BACKTICK) {
				stack.pop()
				last = c
				i += 1
			} else if (c === 36 && src.charCodeAt(i + 1) === 123) {
				stack.push('${')
				i += 2
			} else {
				i += 1
			}
			continue
		}
		if (c === SLASH && src.charCodeAt(i + 1) === SLASH) {
			i = lineEnd(src, i)
			continue
		}
		if (c === SLASH && src.charCodeAt(i + 1) === STAR) {
			i = blockCommentEnd(src, i)
			continue
		}
		if (c === 39 || c === 34) {
			i = stringEnd(src, i + 1, c)
			last = c
			continue
		}
		if (c === BACKTICK) {
			stack.push('`')
			last = c
			i += 1
			continue
		}
		if (c === 40 || c === 91 || c === 123) {
			stack.push(String.fromCharCode(c) as Frame)
			last = c
			i += 1
			continue
		}
		if (c === 41 || c === 93 || c === 125) {
			if (c === 125 && top === '${') stack.pop()
			else closeFrame(stack, OPENER[c] as Frame)
			last = c
			i += 1
			continue
		}
		if (stack.length === 0 && c === 59) {
			finish(i)
			start = i + 1
			i += 1
			continue
		}
		if (stack.length === 0 && c === NEWLINE) {
			if (last >= 0 && !oneOf(CONTINUES_AFTER, last)) {
				const next = nextCode(src, i + 1)
				if (!oneOf(CONTINUES_BEFORE, next)) {
					finish(i)
					start = i + 1
				}
			}
			i += 1
			continue
		}
		if (!isSpace(c)) last = c
		i += 1
	}
	finish(src.length)
	return out
}

/** Pop to the matching opener. A stray closer is ignored. */
function closeFrame(stack: Frame[], opener: Frame): void {
	for (let k = stack.length - 1; k >= 0; k -= 1) {
		const frame = stack[k]
		if (frame === '`' || frame === '${') return
		if (frame === opener) {
			stack.length = k
			return
		}
	}
}

function isSpace(c: number): boolean {
	return c === 32 || (c >= 9 && c <= 13) || c === 0xa0 || c === 0xfeff
}

function lineEnd(src: string, from: number): number {
	const at = src.indexOf('\n', from)
	return at < 0 ? src.length : at
}

function blockCommentEnd(src: string, from: number): number {
	const at = src.indexOf('*/', from + 2)
	return at < 0 ? src.length : at + 2
}

/** After the closing quote. An unterminated string stops before the newline. */
function stringEnd(src: string, from: number, quote: number): number {
	let i = from
	while (i < src.length) {
		const c = src.charCodeAt(i)
		if (c === BACKSLASH) {
			i += 2
			continue
		}
		if (c === quote) return i + 1
		if (c === NEWLINE) return i
		i += 1
	}
	return src.length
}

/** First code character at or after `from`, skipping space and comments. */
function nextCode(src: string, from: number): number {
	let i = from
	while (i < src.length) {
		const c = src.charCodeAt(i)
		if (isSpace(c)) {
			i += 1
		} else if (c === SLASH && src.charCodeAt(i + 1) === SLASH) {
			i = lineEnd(src, i)
		} else if (c === SLASH && src.charCodeAt(i + 1) === STAR) {
			i = blockCommentEnd(src, i)
		} else {
			return c
		}
	}
	return -1
}

function pushTrimmed(out: StatementSpan[], src: string, start: number, end: number): void {
	let from = start
	let to = end
	while (from < to && isSpace(src.charCodeAt(from))) from += 1
	while (to > from && isSpace(src.charCodeAt(to - 1))) to -= 1
	if (from < to) out.push({ start: from, end: to, text: src.slice(from, to) })
}
