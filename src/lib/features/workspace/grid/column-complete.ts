/** Column-name completion for the grid WHERE box. */

export function columnCompletions(
	code: string,
	caret: number,
	columns: readonly string[],
): { start: number; items: string[] } {
	const before = code.slice(0, Math.max(0, caret))
	const match = /[A-Za-z_][A-Za-z0-9_]*$/.exec(before)
	if (!match?.[0]) return { start: caret, items: [] }
	const prefix = match[0]
	const lower = prefix.toLowerCase()
	const items = columns
		.filter((column) => {
			const name = column.toLowerCase()
			return name.startsWith(lower) && name !== lower
		})
		.slice(0, 20)
	return { start: caret - prefix.length, items }
}

export type DistinctCacheEntry = { at: number; values: string[] }

export const DISTINCT_TTL_MS = 30_000
export const DISTINCT_LIMIT = 1000

export function distinctCacheKey(
	sessionId: string,
	table: string,
	column: string,
): string {
	return `${sessionId}\n${table}\n${column}`
}

export function readDistinctCache(
	cache: ReadonlyMap<string, DistinctCacheEntry>,
	key: string,
	now: number,
	ttl = DISTINCT_TTL_MS,
): string[] | null {
	const entry = cache.get(key)
	if (!entry) return null
	if (now - entry.at > ttl) return null
	return entry.values
}
