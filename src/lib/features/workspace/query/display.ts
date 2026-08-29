/** One-line query previews and human-readable engine errors. */

export function previewSql(sql: string, max = 72): string {
	const one = sql.replace(/\s+/g, ' ').trim()
	if (one.length <= max) return one
	return `${one.slice(0, max - 1)}…`
}

/** Strip IPC / driver wrappers so the results pane shows the engine text. */
export function userFacingQueryError(raw: string): string {
	let s = raw.trim()
	s = s.replace(/^connector error:\s*/i, '')
	s = s.replace(/^query failed:\s*/i, '')
	s = s.replace(/^db error(?:\s*[—:\-]\s*)?/i, '')
	s = s.trim()
	return s || 'The database rejected this query.'
}
