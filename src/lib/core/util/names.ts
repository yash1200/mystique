/**
 * `schema.table` → the qualifier with its dot, and the name. A name without
 * a qualifier comes back whole. Splits at the first dot: schemas rarely hold
 * one, and a table that does is still shown in full.
 */
export function splitQualifiedName(full: string): { schema: string; name: string } {
	const dot = full.indexOf('.')
	return dot > 0
		? { schema: full.slice(0, dot + 1), name: full.slice(dot + 1) }
		: { schema: '', name: full }
}

/** Natural, case-blind order by `name`, then `id` so ties stay stable. */
export function byName(a: { name: string; id: string }, b: { name: string; id: string }): number {
	return (
		a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' }) ||
		a.id.localeCompare(b.id)
	)
}
