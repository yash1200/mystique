/**
 * Appearance + session prefs. State is reactive; values persist to localStorage.
 */
export type ThemePref = 'light' | 'dark' | 'system'
export type DensityPref = 'compact' | 'comfortable'
export type ConnectionListSort = 'name' | 'created' | 'engine' | 'environment'

const THEME_KEY = 'mystique-theme'
const DENSITY_KEY = 'mystique-density'
const LAST_CONN_KEY = 'mystique-last-connection-id'
const REMEMBER_KEY = 'mystique-remember-last-connection'
const INSPECT_ON_CLICK_KEY = 'mystique-inspect-on-row-click'
const CONN_SORT_KEY = 'mystique-connection-sort'
const EXPLORER_WIDTH_KEY = 'mystique-explorer-panel-width'

export const EXPLORER_PANEL_WIDTH_DEFAULT = 240
export const EXPLORER_PANEL_WIDTH_MIN = 180
export const EXPLORER_PANEL_WIDTH_MAX = 480

function lsGet(key: string): string | null {
	try {
		return localStorage.getItem(key)
	} catch {
		return null
	}
}

function lsSet(key: string, value: string) {
	try {
		localStorage.setItem(key, value)
	} catch {
		/* ignore */
	}
}

function readTheme(): ThemePref {
	const raw = lsGet(THEME_KEY)
	if (raw === 'light' || raw === 'dark' || raw === 'system') return raw
	return 'dark'
}

function readDensity(): DensityPref {
	const raw = lsGet(DENSITY_KEY)
	return raw === 'comfortable' ? 'comfortable' : 'compact'
}

function readBool(key: string, fallback: boolean): boolean {
	const raw = lsGet(key)
	if (raw == null) return fallback
	return raw === '1' || raw === 'true'
}

function readSort(): ConnectionListSort {
	const raw = lsGet(CONN_SORT_KEY)
	if (
		raw === 'name' ||
		raw === 'created' ||
		raw === 'engine' ||
		raw === 'environment'
	) {
		return raw
	}
	return 'name'
}

export function clampExplorerPanelWidth(px: number): number {
	if (!Number.isFinite(px)) return EXPLORER_PANEL_WIDTH_DEFAULT
	return Math.min(
		EXPLORER_PANEL_WIDTH_MAX,
		Math.max(EXPLORER_PANEL_WIDTH_MIN, Math.round(px)),
	)
}

function readExplorerWidth(): number {
	const raw = lsGet(EXPLORER_WIDTH_KEY)
	if (raw == null || raw === '') return EXPLORER_PANEL_WIDTH_DEFAULT
	return clampExplorerPanelWidth(Number(raw))
}

function systemPrefersDark(): boolean {
	if (typeof window === 'undefined') return true
	return window.matchMedia('(prefers-color-scheme: dark)').matches
}

class AppPrefsStore {
	theme = $state<ThemePref>(readTheme())
	density = $state<DensityPref>(readDensity())
	inspectOnRowClick = $state(readBool(INSPECT_ON_CLICK_KEY, true))
	lastConnectionId = $state<string | null>(lsGet(LAST_CONN_KEY))
	rememberLastConnection = $state(readBool(REMEMBER_KEY, true))
	connectionListSort = $state<ConnectionListSort>(readSort())
	explorerPanelWidth = $state(readExplorerWidth())
	systemDark = $state(systemPrefersDark())

	get resolvedTheme(): 'light' | 'dark' {
		if (this.theme === 'system') return this.systemDark ? 'dark' : 'light'
		return this.theme
	}

	/** Compact 28px / comfortable 32px — keep in sync with `--grid-row-height`. */
	get gridRowHeightPx(): number {
		return this.density === 'comfortable' ? 32 : 28
	}

	apply() {
		if (typeof document === 'undefined') return
		document.documentElement.dataset.theme = this.resolvedTheme
		document.documentElement.dataset.density = this.density
		const bg = getComputedStyle(document.documentElement)
			.getPropertyValue('--bg-app')
			.trim()
		const meta = document.querySelector('meta[name="theme-color"]')
		if (meta && bg) meta.setAttribute('content', bg)
	}

	setTheme(pref: ThemePref) {
		this.theme = pref
		lsSet(THEME_KEY, pref)
		this.apply()
	}

	setDensity(pref: DensityPref) {
		this.density = pref
		lsSet(DENSITY_KEY, pref)
		this.apply()
	}

	setInspectOnRowClick(on: boolean) {
		this.inspectOnRowClick = on
		lsSet(INSPECT_ON_CLICK_KEY, on ? '1' : '0')
	}

	setLastConnectionId(id: string) {
		this.lastConnectionId = id
		lsSet(LAST_CONN_KEY, id)
	}

	setRememberLastConnection(on: boolean) {
		this.rememberLastConnection = on
		lsSet(REMEMBER_KEY, on ? '1' : '0')
	}

	setConnectionListSort(sort: ConnectionListSort) {
		this.connectionListSort = sort
		lsSet(CONN_SORT_KEY, sort)
	}

	setExplorerPanelWidth(px: number, persist = true) {
		this.explorerPanelWidth = clampExplorerPanelWidth(px)
		if (persist) lsSet(EXPLORER_WIDTH_KEY, String(this.explorerPanelWidth))
	}

	watchSystemTheme(): () => void {
		if (typeof window === 'undefined') return () => {}
		const mq = window.matchMedia('(prefers-color-scheme: dark)')
		const sync = () => {
			this.systemDark = mq.matches
			if (this.theme === 'system') this.apply()
		}
		sync()
		mq.addEventListener('change', sync)
		return () => mq.removeEventListener('change', sync)
	}
}

export const appPrefs = new AppPrefsStore()
