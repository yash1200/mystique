import type { SqlRange, StatementResult } from '#core/contracts'

export function statementChipLabel(
	all: Pick<StatementResult, 'index'>[],
	stmt: Pick<StatementResult, 'index' | 'setIndex'>,
): string {
	const n = stmt.index + 1
	const sets = all.filter((item) => item.index === stmt.index).length
	if (sets > 1) return `${n}.${(stmt.setIndex ?? 0) + 1}`
	return String(n)
}

/** Last statement, or the one that stopped the batch. */
export function initialActiveIndex(
	results: Pick<StatementResult, 'index' | 'error'>[],
	stoppedAt: number | null | undefined,
): number {
	if (results.length === 0) return 0
	if (stoppedAt != null) {
		const failed = results.findIndex(
			(item) => item.index === stoppedAt && item.error,
		)
		if (failed >= 0) return failed
		const first = results.findIndex((item) => item.index === stoppedAt)
		if (first >= 0) return first
	}
	return results.length - 1
}

export function statementChipTitle(sql: string, range: SqlRange | null | undefined): string {
	if (!range) return ''
	const slice = sql.slice(range.start, range.end).replace(/\s+/g, ' ').trim()
	if (slice.length <= 80) return slice
	return `${slice.slice(0, 79)}…`
}
