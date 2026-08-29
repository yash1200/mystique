/** App-wide display preferences persisted in the local app DB. */
export type ThemePref = 'light' | 'dark' | 'system'
export type DensityPref = 'compact' | 'comfortable'
export type ConnectionListSort =
	| 'name'
	| 'created'
	| 'recent'
	| 'engine'
	| 'environment'
