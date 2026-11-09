/**
 * Value completion for the filter bar: where a value is being typed in the
 * code filter, and which distinct values match it.
 */

/** Suggestions shown at once. */
export const VALUE_SHOWN = 20
/** Longer values are not offered (they would not fit the list). */
const VALUE_MAX_CHARS = 200

const OPERATORS = '(?:=|!=|<>|>=|<=|>|<|\\bcontains\\b|\\bstarts with\\b|\\bends with\\b)'
const QUOTED = new RegExp(`([A-Za-z_][\\w.]*)\\s*${OPERATORS}\\s*'([^'\\n]*)$`, 'i')
const BARE = new RegExp(`([A-Za-z_][\\w.]*)\\s*${OPERATORS}\\s*([^'\\s]*)$`, 'i')

export type CodeValueContext = {
	column: string
	/** Replace `[start, end)` with the picked value, quoted. */
	start: number
	end: number
	prefix: string
}

/** `col = 'pa|` or `col = pa|` at the caret, for a known column. */
export function codeValueContext(
	code: string,
	caret: number,
	columns: readonly string[],
): CodeValueContext | null {
	const at = Math.max(0, Math.min(caret, code.length))
	const before = code.slice(0, at)
	const quoted = QUOTED.exec(before)
	const match = quoted ?? BARE.exec(before)
	if (!match?.[1]) return null
	const typed = match[1].toLowerCase()
	const column = columns.find((name) => name.toLowerCase() === typed)
	if (!column) return null
	const prefix = match[2] ?? ''
	const start = at - prefix.length - (quoted ? 1 : 0)
	let end = at
	if (quoted) {
		const close = code.indexOf("'", at)
		const newline = code.indexOf('\n', at)
		if (close >= 0 && (newline < 0 || close < newline)) end = close + 1
	} else {
		while (end < code.length && !/\s/.test(code.charAt(end))) end += 1
	}
	return { column, start, end, prefix }
}

/** Put `'value'` in place of the typed value. */
export function insertCodeValue(
	code: string,
	ctx: CodeValueContext,
	value: string,
): { code: string; caret: number } {
	const quoted = `'${value}'`
	return {
		code: code.slice(0, ctx.start) + quoted + code.slice(ctx.end),
		caret: ctx.start + quoted.length,
	}
}

/** Values that start with `prefix` (ASCII case-insensitive), the typed one left out. */
export function matchValues(
	values: readonly string[],
	prefix: string,
	limit = VALUE_SHOWN,
): string[] {
	const lower = prefix.toLowerCase()
	const out: string[] = []
	for (const value of values) {
		if (value.length > VALUE_MAX_CHARS || value.includes('\n')) continue
		if (value === prefix) continue
		if (lower && !value.toLowerCase().startsWith(lower)) continue
		out.push(value)
		if (out.length >= limit) break
	}
	return out
}
