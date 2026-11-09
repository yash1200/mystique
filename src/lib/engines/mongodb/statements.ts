/**
 * Mongo shell statements in the editor: the run-at-cursor range and the gutter
 * run markers. Copy of `crates/plugins/mystique-plugin-mongodb/src/shell/split.rs`,
 * which cuts Run all on the backend; both are held to
 * `crates/plugins/mystique-plugin-mongodb/tests/fixtures/statements/cases.json`.
 *
 * A statement ends at a `;` outside brackets, or at a line break outside
 * brackets, unless the line ends with a character that needs more
 * (`CONTINUES_LINE`) or the next line starts with one that cannot begin a
 * statement (`CONTINUES_NEXT`, e.g. `.find()`). Strings and comments follow the
 * shell lexer: an unclosed `'` / `"` ends at the line break, a template literal
 * spans lines. A range runs from the first to the last code character, so
 * comments, whitespace and the `;` around it are outside.
 *
 * Offsets are JavaScript string indexes. Rust reports byte offsets; the corpus
 * compares statement text, so non-ASCII buffers agree on text, not offsets.
 */

export type MongoStatementSpan = {
	start: number
	end: number
	text: string
}

const CONTINUES_LINE = '.,([{:?+-*/%&|^!~=<>'
const CONTINUES_NEXT = '.,)]}:?+-*/%&|^=<>'

/** `ch` is one of the characters in `set`. An empty `ch` is not. */
function oneOf(set: string, ch: string): boolean {
	return ch.length === 1 && set.includes(ch)
}

type Piece ={ kind: 'code'; ch: string } | { kind: 'str' } | { kind: 'newline' }

/** Rust `char::is_whitespace`: JS `\s` without U+FEFF, plus U+0085. */
function isSpace(ch: string): boolean {
	if (ch === '\u0085') return true
	if (ch === '﻿') return false
	return /\s/.test(ch)
}

/** Walk `text` with the shell lexer's rules for strings and comments. */
function skim(
	text: string,
	visit: (piece: Piece, start: number, end: number) => void,
): void {
	let i = 0
	while (i < text.length) {
		const at = i
		const cp = text.codePointAt(i) ?? 0
		const ch = String.fromCodePoint(cp)
		i += ch.length
		if (ch === '\n') {
			visit({ kind: 'newline' }, at, i)
			continue
		}
		if (isSpace(ch)) continue
		if (ch === '/') {
			const next = text[i]
			if (next === '/') {
				while (i < text.length && text[i] !== '\n') i += 1
				continue
			}
			if (next === '*') {
				i += 1
				const close = text.indexOf('*/', i)
				const end = close < 0 ? text.length : close + 2
				const spansLines = text.slice(i, close < 0 ? end : close).includes('\n')
				i = end
				if (spansLines) visit({ kind: 'newline' }, at, end)
				continue
			}
		}
		if (ch === '"' || ch === "'" || ch === '`') {
			let end = text.length
			while (i < text.length) {
				const n = text[i]
				if (n === '\n' && ch !== '`') {
					end = i
					break
				}
				i += 1
				if (n === ch) {
					end = i
					break
				}
				if (n === '\\' && i < text.length) {
					i += String.fromCodePoint(text.codePointAt(i) ?? 0).length
				}
			}
			visit({ kind: 'str' }, at, end)
			continue
		}
		visit({ kind: 'code', ch }, at, i)
	}
}

/** Each statement in `text`, in order. Empty and comment-only chunks are skipped. */
export function splitMongoStatements(text: string): MongoStatementSpan[] {
	const ranges: Array<[number, number]> = []
	let current: [number, number] | null = null
	let depth = 0
	let last = ''
	let lineBreak = false
	skim(text, (piece, start, end) => {
		if (piece.kind === 'newline') {
			if (depth === 0 && current) lineBreak = true
			return
		}
		if (piece.kind === 'code' && piece.ch === ';' && depth === 0) {
			if (current) ranges.push(current)
			current = null
			lineBreak = false
			return
		}
		const first = piece.kind === 'code' ? piece.ch : '"'
		if (lineBreak && !oneOf(CONTINUES_LINE, last) && !oneOf(CONTINUES_NEXT, first)) {
			if (current) ranges.push(current)
			current = null
		}
		lineBreak = false
		if (current) current[1] = end
		else current = [start, end]
		last = first
		if (first === '(' || first === '[' || first === '{') depth += 1
		else if (first === ')' || first === ']' || first === '}') {
			depth = Math.max(0, depth - 1)
		}
	})
	if (current) ranges.push(current)
	return ranges.map(([start, end]) => ({ start, end, text: text.slice(start, end) }))
}
