import type { AccessLevel } from './mcp-types'

export const ACCESS_LEVELS: readonly AccessLevel[] = [
	'none',
	'read',
	'write',
	'full',
] as const

const LABELS: Record<AccessLevel, string> = {
	none: 'None',
	read: 'Read',
	write: 'Write',
	full: 'Full',
}

const HINTS: Record<AccessLevel, string> = {
	none: 'Hidden. Tools cannot see it.',
	read: 'SELECT and find only.',
	write: 'Adds INSERT and UPDATE.',
	full: 'Adds DELETE, DDL and DROP.',
}

export function isAccessLevel(value: string): value is AccessLevel {
	return (ACCESS_LEVELS as readonly string[]).includes(value)
}

export function levelLabel(value: string): string {
	return isAccessLevel(value) ? LABELS[value] : value
}

export function levelHint(value: string): string {
	return isAccessLevel(value) ? HINTS[value] : ''
}

/** The value a picker uses for "no level set here". */
export const INHERIT = 'inherit'

/** What an unset connection or database inherits, for the picker label. */
export function inheritedLabel(mcp: string, agent: string): string {
	return mcp === agent
		? levelLabel(mcp)
		: `MCP ${levelLabel(mcp)}, agent ${levelLabel(agent)}`
}

/** Options for a connection or database picker: inherit, then the levels. */
export function levelOptions(
	inherited: string,
): { value: string; label: string }[] {
	return [
		{ value: INHERIT, label: `Inherit (${inherited})` },
		...ACCESS_LEVELS.map((l) => ({ value: l, label: LABELS[l] })),
	]
}
