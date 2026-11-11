/** Checks a new name for Rename / Duplicate before it reaches the engine. */
export function tableNameError(input: {
	action: 'rename' | 'duplicate'
	current: string
	next: string
	noun: string
	schema?: string | null
	/** Names already in that schema. */
	taken: readonly string[]
}): string | null {
	const name = input.next.trim()
	if (!name) return `Give the ${input.noun} a name.`
	if (name === input.current) {
		return input.action === 'rename' ? null : `Pick a name other than ${input.current}.`
	}
	const lower = name.toLowerCase()
	if (!input.taken.some((t) => t.toLowerCase() === lower)) return null
	return `${input.schema ? `${input.schema}.` : ''}${name} already exists.`
}
