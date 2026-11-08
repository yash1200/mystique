/**
 * Value sent in an UPDATE/DELETE predicate.
 * Server cells stay as they arrived. Display text is not parsed again:
 * a text value `"x"` must not become `x`.
 */
export function identityCell(v: unknown): unknown {
	if (v == null) return null
	if (typeof v === 'object') {
		const o = v as Record<string, unknown>
		if (typeof o.$oid === 'string') return o.$oid
		if (typeof o.$uuid === 'string') return o.$uuid
	}
	return v
}
