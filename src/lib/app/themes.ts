/**
 * The colour themes. Each pairs a neutral ramp with an accent ramp and is
 * dark or light; Light / Dark / System picks which one is on screen, and the
 * user picks one theme of each kind. Values live in `styles/tokens.css`
 * (`[data-palette]`; the defaults, Night and Day, carry none).
 */
export type ThemeScheme = 'dark' | 'light'
export type DarkTheme = 'night' | 'abyss'
export type LightTheme = 'day' | 'linen'
export type ThemeId = DarkTheme | LightTheme

export type ThemeInfo = {
	id: ThemeId
	name: string
	/** Neutral × accent. */
	ramps: string
	scheme: ThemeScheme
}

export const THEMES: readonly ThemeInfo[] = [
	{ id: 'night', name: 'Night', ramps: 'Graphite × Mint', scheme: 'dark' },
	{ id: 'abyss', name: 'Abyss', ramps: 'Navy × Iris', scheme: 'dark' },
	{ id: 'day', name: 'Day', ramps: 'Graphite × Mint', scheme: 'light' },
	{ id: 'linen', name: 'Linen', ramps: 'Warm paper × Ember', scheme: 'light' },
]

export const DEFAULT_THEME: Record<ThemeScheme, ThemeId> = { dark: 'night', light: 'day' }

export function themesFor(scheme: ThemeScheme): ThemeInfo[] {
	return THEMES.filter((t) => t.scheme === scheme)
}

export function parseDarkTheme(raw: unknown): DarkTheme {
	return raw === 'abyss' ? 'abyss' : 'night'
}

export function parseLightTheme(raw: unknown): LightTheme {
	return raw === 'linen' ? 'linen' : 'day'
}

/** The `data-palette` value for a theme: none for the defaults. */
export function paletteAttr(theme: ThemeId): string | undefined {
	return theme === 'night' || theme === 'day' ? undefined : theme
}
