/**
 * Editor-side copy of `mystique-sql` `split.rs`.
 * Offsets are JavaScript string indexes, which match the Rust byte offsets
 * for the ASCII batches the splitter tests lock.
 */

export type SplitDialect = 'postgres' | 'mysql' | 'sqlite' | 'generic'

export type StatementSpan = {
	start: number
	end: number
	text: string
}

export function splitDialectFor(engine: string): SplitDialect {
	const id = engine.trim().toLowerCase()
	if (
		id === 'postgres' ||
		id === 'postgresql' ||
		id === 'cockroachdb' ||
		id === 'pg'
	) {
		return 'postgres'
	}
	if (id === 'mysql' || id === 'mariadb' || id === 'tidb') return 'mysql'
	if (id === 'sqlite' || id === 'turso') return 'sqlite'
	return 'generic'
}

export function splitStatementSpans(
	sql: string,
	dialect: SplitDialect,
): StatementSpan[] {
	const out: StatementSpan[] = []
	let start = 0
	let i = 0
	let inSingle = false
	let inDouble = false
	let inBacktick = false
	let dollar: string | null = null

	while (i < sql.length) {
		const b = sql.charCodeAt(i)
		if (dollar) {
			if (sql.startsWith(dollar, i)) {
				i += dollar.length
				dollar = null
				continue
			}
			i += 1
			continue
		}
		if (inSingle) {
			if (b === 39) {
				if (sql.charCodeAt(i + 1) === 39) {
					i += 2
					continue
				}
				inSingle = false
			}
			i += 1
			continue
		}
		if (inDouble) {
			if (b === 34) {
				if (sql.charCodeAt(i + 1) === 34) {
					i += 2
					continue
				}
				inDouble = false
			}
			i += 1
			continue
		}
		if (inBacktick) {
			if (b === 96) {
				if (sql.charCodeAt(i + 1) === 96) {
					i += 2
					continue
				}
				inBacktick = false
			}
			i += 1
			continue
		}
		if (b === 45 && sql.charCodeAt(i + 1) === 45) {
			i += 2
			while (i < sql.length && sql.charCodeAt(i) !== 10) i += 1
			continue
		}
		if (b === 35 && dialect === 'mysql') {
			while (i < sql.length && sql.charCodeAt(i) !== 10) i += 1
			continue
		}
		if (b === 47 && sql.charCodeAt(i + 1) === 42) {
			i += 2
			while (
				i + 1 < sql.length &&
				!(sql.charCodeAt(i) === 42 && sql.charCodeAt(i + 1) === 47)
			) {
				i += 1
			}
			i = Math.min(i + 2, sql.length)
			continue
		}
		if (b === 36) {
			const tagEnd = dollarTagEnd(sql, i)
			if (tagEnd != null) {
				dollar = sql.slice(i, tagEnd)
				i = tagEnd
				continue
			}
		}
		if (b === 39) inSingle = true
		else if (b === 34) inDouble = true
		else if (b === 96) inBacktick = true
		else if (b === 59) {
			pushSpan(out, sql, start, i)
			start = i + 1
		}
		i += 1
	}
	pushSpan(out, sql, start, sql.length)
	return out
}

export function statementAt(
	spans: StatementSpan[],
	cursor: number,
): StatementSpan | null {
	if (spans.length === 0) return null
	for (let i = 0; i < spans.length; i += 1) {
		const span = spans[i]
		if (!span) continue
		const regionStart = i === 0 ? 0 : (spans[i - 1]?.end ?? 0)
		const regionEnd = spans[i + 1]?.start ?? Number.POSITIVE_INFINITY
		if (cursor >= regionStart && cursor < regionEnd) return span
	}
	return spans[spans.length - 1] ?? null
}

/** Text ⌘Enter / ⌘⇧Enter should send. Empty selection or buffer returns null. */
export function textToRun(
	sql: string,
	dialect: SplitDialect,
	head: number,
	anchor: number,
	all: boolean,
): string | null {
	if (all) {
		const whole = sql.trim()
		return whole || null
	}
	if (anchor !== head) {
		const from = Math.min(anchor, head)
		const to = Math.max(anchor, head)
		const selected = sql.slice(from, to).trim()
		return selected || null
	}
	return statementAt(splitStatementSpans(sql, dialect), head)?.text ?? null
}

function pushSpan(
	out: StatementSpan[],
	sql: string,
	start: number,
	end: number,
): void {
	const raw = sql.slice(start, end)
	const trimmedStart = raw.trimStart()
	const lead = raw.length - trimmedStart.length
	const trimmed = trimmedStart.trimEnd()
	if (!trimmed || isCommentOnly(trimmed)) return
	const spanStart = start + lead
	const spanEnd = end - (trimmedStart.length - trimmed.length)
	out.push({ start: spanStart, end: spanEnd, text: trimmed })
}

function isCommentOnly(sql: string): boolean {
	const t = sql.trim()
	if (!t) return true
	if (t.startsWith('--') && !t.includes('\n')) return true
	return (
		t.startsWith('/*') &&
		t.endsWith('*/') &&
		!t.slice(2, -2).includes('*/')
	)
}

function dollarTagEnd(sql: string, i: number): number | null {
	let j = i + 1
	while (j < sql.length) {
		const c = sql.charCodeAt(j)
		const ok =
			(c >= 48 && c <= 57) ||
			(c >= 65 && c <= 90) ||
			(c >= 97 && c <= 122) ||
			c === 95
		if (!ok) break
		j += 1
	}
	if (sql.charCodeAt(j) === 36) return j + 1
	return null
}
