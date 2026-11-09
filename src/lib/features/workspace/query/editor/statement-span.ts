/**
 * Which text run-at-cursor sends, and where the statements are.
 *
 * SQL tabs split like `mystique-sql` (`splitSqlStatements`, locked by the
 * shared fixture corpus). Mongo tabs use the shell statement model instead:
 * newlines and brackets matter there, not `BEGIN … END`.
 */
import {
	splitMongoStatements,
	splitSqlStatements,
	sqlSplitDialectFor,
	type SqlSplitDialect,
	type StatementSpan,
} from '#engines'

export type { StatementSpan }

/** SQL dialect for the splitter and the parameter scan. */
export type SplitDialect = SqlSplitDialect

/** How a tab's text breaks into statements. */
export type StatementModel = SqlSplitDialect | 'mongo'

export function splitDialectFor(engine: string): SplitDialect {
	return sqlSplitDialectFor(engine)
}

/** `mql` tabs use the Mongo shell model; everything else splits as SQL. */
export function statementModelFor(engine: string, language: string): StatementModel {
	return language === 'mql' ? 'mongo' : sqlSplitDialectFor(engine)
}

let memo: { sql: string; model: StatementModel; spans: StatementSpan[] } | null = null

/**
 * Statements in `sql`. The last result is kept: the gutter, run-at-cursor
 * and statement navigation all ask about the same document.
 */
export function splitStatementSpans(sql: string, model: StatementModel): StatementSpan[] {
	if (memo && memo.sql === sql && memo.model === model) return memo.spans
	const spans = model === 'mongo' ? splitMongoStatements(sql) : splitSqlStatements(sql, model)
	memo = { sql, model, spans }
	return spans
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

/** Index of the statement whose region holds `cursor`, or -1. */
export function statementIndexAt(spans: StatementSpan[], cursor: number): number {
	const hit = statementAt(spans, cursor)
	return hit ? spans.indexOf(hit) : -1
}

/**
 * Where statement navigation lands from `cursor`: the start of the next
 * statement, or of the previous one. From inside a statement, "previous"
 * first goes to that statement's own start. Null when there is nowhere to go.
 */
export function neighbourStatement(
	spans: StatementSpan[],
	cursor: number,
	direction: 1 | -1,
): StatementSpan | null {
	if (direction === 1) return spans.find((span) => span.start > cursor) ?? null
	for (let i = spans.length - 1; i >= 0; i -= 1) {
		const span = spans[i]
		if (span && span.start < cursor) return span
	}
	return null
}

/** Text ⌘Enter / ⌘⇧Enter should send. Empty selection or buffer returns null. */
export function textToRun(
	sql: string,
	model: StatementModel,
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
	return statementAt(splitStatementSpans(sql, model), head)?.text ?? null
}

/** Editor index of the text [`textToRun`] returns. */
export function submissionStart(
	sql: string,
	model: StatementModel,
	head: number,
	anchor: number,
	all: boolean,
): number {
	if (all) return sql.length - sql.trimStart().length
	if (anchor !== head) {
		const from = Math.min(anchor, head)
		const raw = sql.slice(from, Math.max(anchor, head))
		return from + (raw.length - raw.trimStart().length)
	}
	return statementAt(splitStatementSpans(sql, model), head)?.start ?? 0
}
