/** Client-side pages over a fetched result. */
export type ResultPage<Row> = {
	total: number
	totalPages: number
	/** `page` clamped to the last page. */
	page: number
	start: number
	end: number
	rows: Row[]
}

export function resultPage<Row>(
	rows: Row[],
	page: number,
	pageSize: number,
): ResultPage<Row> {
	const total = rows.length
	const totalPages = Math.max(1, Math.ceil(total / pageSize) || 1)
	const safe = Math.min(page, totalPages - 1)
	const start = safe * pageSize
	const end = Math.min(start + pageSize, total)
	return { total, totalPages, page: safe, start, end, rows: rows.slice(start, end) }
}
