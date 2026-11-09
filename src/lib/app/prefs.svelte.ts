import { isMacPlatform } from '#core/util'
import { INSPECT_ON_ROW_CLICK_DEFAULT, parseBoolPref } from './prefs-defaults'
import { parseTimeoutSecs, QUERY_TIMEOUT_DEFAULT_SECS } from './query-timeout'
import {
	parseReviewBeforeSaving,
	type ConnectionListSort,
	type DensityPref,
	type ReviewBeforeSaving,
	type SavedQueryListSort,
	type ThemePref,
} from './prefs-types'

export type {
	ConnectionListSort,
	DensityPref,
	ReviewBeforeSaving,
	SavedQueryListSort,
	ThemePref,
}

/**
 * Appearance + session prefs. State is reactive; values persist to localStorage.
 */

const THEME_KEY = 'mystique-theme'
const DENSITY_KEY = 'mystique-density'
const INSPECT_ON_CLICK_KEY = 'mystique-inspect-on-row-click'
const OPEN_TABLE_SINGLE_CLICK_KEY = 'mystique-open-table-on-single-click'
const CONN_SORT_KEY = 'mystique-connection-sort'
const QUERY_SORT_KEY = 'mystique-saved-query-sort'
const EXPLORER_WIDTH_KEY = 'mystique-explorer-panel-width'
const HOME_SIDEBAR_WIDTH_KEY = 'mystique-home-sidebar-width'
const AGENT_CHAT_OPEN_KEY = 'mystique-agent-chat-open'
const AGENT_CHAT_WIDTH_KEY = 'mystique-agent-chat-panel-width'
const REVIEW_SAVE_KEY = 'mystique-review-before-saving'
const GRID_SQL_PANEL_KEY = 'mystique-grid-sql-panel'
const QUERY_TIMEOUT_KEY = 'mystique-query-timeout-secs'

export const EXPLORER_PANEL_WIDTH_DEFAULT = 240
export const EXPLORER_PANEL_WIDTH_MIN = 180
export const EXPLORER_PANEL_WIDTH_MAX = 480
/** Matches current `w-sidebar` (17.5rem) so first-run size does not jump. */
export const HOME_SIDEBAR_WIDTH_DEFAULT = 280
export const HOME_SIDEBAR_WIDTH_MIN = 200
export const HOME_SIDEBAR_WIDTH_MAX = 420
export const AGENT_CHAT_PANEL_WIDTH_DEFAULT = 320
export const AGENT_CHAT_PANEL_WIDTH_MIN = 280
export const AGENT_CHAT_PANEL_WIDTH_MAX = 720

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

function lsRemove(key: string) {
	try {
		localStorage.removeItem(key)
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
	return parseBoolPref(lsGet(key), fallback)
}

function readSort(): ConnectionListSort {
	const raw = lsGet(CONN_SORT_KEY)
	if (
		raw === 'name' ||
		raw === 'created' ||
		raw === 'recent' ||
		raw === 'engine' ||
		raw === 'environment'
	) {
		return raw
	}
	return 'created'
}

function readQuerySort(): SavedQueryListSort {
	const raw = lsGet(QUERY_SORT_KEY)
	if (raw === 'name' || raw === 'created' || raw === 'recent') return raw
	return 'created'
}

function clampExplorerPanelWidth(px: number): number {
	if (!Number.isFinite(px)) return EXPLORER_PANEL_WIDTH_DEFAULT
	return Math.min(
		EXPLORER_PANEL_WIDTH_MAX,
		Math.max(EXPLORER_PANEL_WIDTH_MIN, Math.round(px)),
	)
}

function clampHomeSidebarWidth(px: number): number {
	if (!Number.isFinite(px)) return HOME_SIDEBAR_WIDTH_DEFAULT
	return Math.min(
		HOME_SIDEBAR_WIDTH_MAX,
		Math.max(HOME_SIDEBAR_WIDTH_MIN, Math.round(px)),
	)
}

function clampAgentChatPanelWidth(px: number): number {
	if (!Number.isFinite(px)) return AGENT_CHAT_PANEL_WIDTH_DEFAULT
	return Math.min(
		AGENT_CHAT_PANEL_WIDTH_MAX,
		Math.max(AGENT_CHAT_PANEL_WIDTH_MIN, Math.round(px)),
	)
}

function readExplorerWidth(): number {
	const raw = lsGet(EXPLORER_WIDTH_KEY)
	if (raw == null || raw === '') return EXPLORER_PANEL_WIDTH_DEFAULT
	return clampExplorerPanelWidth(Number(raw))
}

function readHomeSidebarWidth(): number {
	const raw = lsGet(HOME_SIDEBAR_WIDTH_KEY)
	if (raw == null || raw === '') return HOME_SIDEBAR_WIDTH_DEFAULT
	return clampHomeSidebarWidth(Number(raw))
}

function readAgentChatWidth(): number {
	const raw = lsGet(AGENT_CHAT_WIDTH_KEY)
	if (raw == null || raw === '') return AGENT_CHAT_PANEL_WIDTH_DEFAULT
	return clampAgentChatPanelWidth(Number(raw))
}

function readQueryTimeout(): number {
	return parseTimeoutSecs(lsGet(QUERY_TIMEOUT_KEY)) ?? QUERY_TIMEOUT_DEFAULT_SECS
}

function systemPrefersDark(): boolean {
	if (typeof window === 'undefined') return true
	return window.matchMedia('(prefers-color-scheme: dark)').matches
}

class AppPrefsStore {
	theme = $state<ThemePref>(readTheme())
	density = $state<DensityPref>(readDensity())
	inspectOnRowClick = $state(readBool(INSPECT_ON_CLICK_KEY, INSPECT_ON_ROW_CLICK_DEFAULT))
	/** macOS defaults to single-click; other platforms default to double-click. */
	openTableOnSingleClick = $state(
		readBool(OPEN_TABLE_SINGLE_CLICK_KEY, isMacPlatform()),
	)
	connectionListSort = $state<ConnectionListSort>(readSort())
	savedQueryListSort = $state<SavedQueryListSort>(readQuerySort())
	explorerPanelWidth = $state(readExplorerWidth())
	homeSidebarWidth = $state(readHomeSidebarWidth())
	/** Session-only. Always starts closed; never written to localStorage. */
	agentChatOpen = $state(false)
	agentChatPanelWidth = $state(readAgentChatWidth())
	systemDark = $state(systemPrefersDark())
	reviewBeforeSaving = $state<ReviewBeforeSaving>(
		parseReviewBeforeSaving(lsGet(REVIEW_SAVE_KEY)),
	)
	/** Grid side panel with the pending statements. */
	gridSqlPanel = $state(readBool(GRID_SQL_PANEL_KEY, false))
	/** Query timeout when the profile sets none. 0 means no timeout. */
	queryTimeoutSecs = $state(readQueryTimeout())

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

	setOpenTableOnSingleClick(on: boolean) {
		this.openTableOnSingleClick = on
		lsSet(OPEN_TABLE_SINGLE_CLICK_KEY, on ? '1' : '0')
	}

	setConnectionListSort(sort: ConnectionListSort) {
		this.connectionListSort = sort
		lsSet(CONN_SORT_KEY, sort)
	}

	setSavedQueryListSort(sort: SavedQueryListSort) {
		this.savedQueryListSort = sort
		lsSet(QUERY_SORT_KEY, sort)
	}

	setExplorerPanelWidth(px: number, persist = true) {
		this.explorerPanelWidth = clampExplorerPanelWidth(px)
		if (persist) lsSet(EXPLORER_WIDTH_KEY, String(this.explorerPanelWidth))
	}

	setHomeSidebarWidth(px: number, persist = true) {
		this.homeSidebarWidth = clampHomeSidebarWidth(px)
		if (persist) lsSet(HOME_SIDEBAR_WIDTH_KEY, String(this.homeSidebarWidth))
	}

	/** Ignores a value that is not a whole number of seconds in range. */
	setQueryTimeoutSecs(raw: unknown) {
		const secs = parseTimeoutSecs(raw)
		if (secs === null) return
		this.queryTimeoutSecs = secs
		lsSet(QUERY_TIMEOUT_KEY, String(secs))
	}

	setAgentChatOpen(on: boolean) {
		this.agentChatOpen = on
		lsRemove(AGENT_CHAT_OPEN_KEY)
	}

	setAgentChatPanelWidth(px: number, persist = true) {
		this.agentChatPanelWidth = clampAgentChatPanelWidth(px)
		if (persist) lsSet(AGENT_CHAT_WIDTH_KEY, String(this.agentChatPanelWidth))
	}

	setReviewBeforeSaving(pref: ReviewBeforeSaving) {
		this.reviewBeforeSaving = pref
		lsSet(REVIEW_SAVE_KEY, pref)
	}

	setGridSqlPanel(on: boolean) {
		this.gridSqlPanel = on
		lsSet(GRID_SQL_PANEL_KEY, on ? '1' : '0')
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
