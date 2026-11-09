/**
 * Catalog fingerprint for the query editor. Kept apart from `complete.ts` so
 * it can be used without loading CodeMirror.
 */
import type { CatalogColumnRow } from '#core/contracts'

/**
 * Stable across a new array that holds the same columns.
 * The editor rebuilds `sql()` only when this changes.
 */
export function catalogSignature(rows: readonly CatalogColumnRow[]): string {
	let hash = 2166136261
	const mix = (text: string) => {
		for (let i = 0; i < text.length; i++) {
			hash ^= text.charCodeAt(i)
			hash = Math.imul(hash, 16777619)
		}
	}
	mix(String(rows.length))
	for (const row of rows) {
		mix(row.schema ?? '')
		mix('\0')
		mix(row.table)
		mix('\0')
		mix(row.column)
		mix('\0')
		mix(row.dataType)
		mix(row.pk ? '1' : '0')
	}
	return (hash >>> 0).toString(16)
}
