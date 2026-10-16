/**
 * Quick Open match tiers, best first:
 * exact, initials, prefix, word-prefix, substring, subsequence.
 * A current-connection hit outranks a tighter match on another connection.
 */

export const MATCH_TIER = {
	exact: 0,
	initials: 1,
	prefix: 2,
	wordPrefix: 3,
	substring: 4,
	subsequence: 5,
} as const

export type MatchTier = (typeof MATCH_TIER)[keyof typeof MATCH_TIER]

const TIER_ORDER: MatchTier[] = [
	MATCH_TIER.exact,
	MATCH_TIER.initials,
	MATCH_TIER.prefix,
	MATCH_TIER.wordPrefix,
	MATCH_TIER.substring,
	MATCH_TIER.subsequence,
]

export function matchTier(query: string, text: string): MatchTier | null {
	const token = query.trim().toLowerCase()
	const raw = text.trim()
	const hay = raw.toLowerCase()
	if (!token || !hay) return null
	if (hay === token) return MATCH_TIER.exact
	const words = wordsOf(raw)
	const initials = words.map((word) => word.charAt(0)).join('')
	if (
		initials.startsWith(token) &&
		(token.length > 1 || initials.length === 1)
	) {
		return MATCH_TIER.initials
	}
	if (hay.startsWith(token)) return MATCH_TIER.prefix
	if (words.some((word) => word.startsWith(token))) return MATCH_TIER.wordPrefix
	if (hay.includes(token)) return MATCH_TIER.substring
	if (isSubsequence(token, hay)) return MATCH_TIER.subsequence
	return null
}

/** Best tier across fields. Every whitespace-separated token must match. */
export function matchTerms(query: string, fields: string[]): MatchTier | null {
	const tokens = query
		.trim()
		.toLowerCase()
		.split(/\s+/)
		.filter((token) => token.length > 0)
	if (tokens.length === 0) return null
	let worst: MatchTier = MATCH_TIER.exact
	for (const token of tokens) {
		let best: MatchTier | null = null
		for (const field of fields) {
			const tier = matchTier(token, field)
			if (tier == null) continue
			if (best == null || tier < best) best = tier
		}
		if (best == null) return null
		if (best > worst) worst = best
	}
	return worst
}

export function tierRank(tier: MatchTier): number {
	return TIER_ORDER.indexOf(tier)
}

function wordsOf(text: string): string[] {
	const spaced = text
		.replace(/([a-z0-9])([A-Z])/g, '$1 $2')
		.replace(/([A-Z]+)([A-Z][a-z])/g, '$1 $2')
	return spaced
		.toLowerCase()
		.split(/[^a-z0-9]+/)
		.filter((word) => word.length > 0)
}

function isSubsequence(token: string, hay: string): boolean {
	let index = 0
	for (const char of hay) {
		if (char === token.charAt(index)) index += 1
		if (index >= token.length) return true
	}
	return false
}
