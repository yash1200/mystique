/** App-wide display preferences persisted in the local app DB. */
export type ThemePref = 'light' | 'dark' | 'system'
export type DensityPref = 'compact' | 'comfortable'
export type ConnectionListSort =
	| 'name'
	| 'created'
	| 'recent'
	| 'engine'
	| 'environment'

export type SavedQueryListSort = 'name' | 'created' | 'recent'

/** When a grid save shows its SQL first. Production always asks once per session. */
export type ReviewBeforeSaving = 'never' | 'production' | 'always'

export function parseReviewBeforeSaving(raw: string | null): ReviewBeforeSaving {
	return raw === 'never' || raw === 'always' ? raw : 'production'
}
