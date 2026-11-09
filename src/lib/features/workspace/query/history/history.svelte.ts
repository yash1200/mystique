/** Query history lists. The connection manager writes the rows. */
import { errorMessage, isTauri } from '#core/ipc'
import { workspaceSession } from '#features/workspace/session'
import {
	historyItemToEntry,
	listQueryHistory,
	listRecentQueryHistory,
	type QueryHistoryPage,
} from './history-api'
import type { QueryHistoryEntry, QueryHistoryItem } from './history-types'

/** One keyset page. The server keeps 2_000 runs per connection. */
const HISTORY_PAGE = 100

/** One connection's history list (or every connection's, with none active). */
export type HistoryList = {
	entries: QueryHistoryEntry[]
	loading: boolean
	loadingMore: boolean
	hasMore: boolean
	error: string | null
	/** A page has been fetched into `entries`. */
	loaded: boolean
	selectedId: string | null
}

const EMPTY: HistoryList = Object.freeze({
	entries: Object.freeze([]) as unknown as QueryHistoryEntry[],
	loading: false,
	loadingMore: false,
	hasMore: false,
	error: null,
	loaded: false,
	selectedId: null,
})

/** List key: the connection id, or '' for the all-connections list. */
function scopeKey(connectionId: string | null | undefined): string {
	return connectionId ?? ''
}

/** History lists keyed by connectionId. */
class QueryHistoryStore {
	lists = $state<Record<string, HistoryList>>({})

	of(connectionId: string | null | undefined): HistoryList {
		return this.lists[scopeKey(connectionId)] ?? EMPTY
	}

	/** The active connection's list. */
	get current(): HistoryList {
		return this.of(workspaceSession.activeConnectionId)
	}

	patch(connectionId: string | null | undefined, next: Partial<HistoryList>) {
		const key = scopeKey(connectionId)
		this.lists = {
			...this.lists,
			[key]: { ...(this.lists[key] ?? EMPTY), ...next },
		}
	}

	select(connectionId: string | null | undefined, id: string | null) {
		this.patch(connectionId, { selectedId: id })
	}

	forget(connectionId: string) {
		if (!(connectionId in this.lists)) return
		const { [connectionId]: _drop, ...rest } = this.lists
		this.lists = rest
	}
}

export const historyStore = new QueryHistoryStore()

/** Fetch generation per list: a newer fetch drops an older one's result. */
const fetchGen = new Map<string, number>()
/** The list loaded last. Switching lists refetches, as it always has. */
let lastLoaded: string | null = null
let historySearch = ''

function genOf(connectionId: string | null): number {
	return fetchGen.get(scopeKey(connectionId)) ?? 0
}

function nextGen(connectionId: string | null): number {
	const gen = genOf(connectionId) + 1
	fetchGen.set(scopeKey(connectionId), gen)
	return gen
}

function historyNameMap(): Map<string, string> {
	return new Map(
		workspaceSession.sessions.map(
			(s) => [s.profile.id, s.profile.name] as const,
		),
	)
}

function entryFromItem(it: QueryHistoryItem) {
	const nameById = historyNameMap()
	return historyItemToEntry(
		it,
		(it.connectionId && nameById.get(it.connectionId)) || undefined,
	)
}

export function openSqlInEditor(sql: string, title?: string) {
	const id = workspaceSession.activeConnectionId
	if (!id) return
	workspaceSession.openNewQueryWithText(id, sql, title)
}

export function selectHistory(id: string) {
	const scope = workspaceSession.activeConnectionId
	historyStore.select(scope, id)
	const h = historyStore.of(scope).entries.find((x) => x.id === id)
	if (!h || !scope) return
	openSqlInEditor(h.sql)
}

export function setHistorySearch(value: string) {
	const next = value.trim()
	if (next === historySearch) return
	historySearch = next
	void refreshHistory({ force: true })
}

export function clearHistorySearch() {
	historySearch = ''
}

export function prependHistoryItem(item: QueryHistoryItem | null | undefined) {
	if (!item?.id) {
		void refreshHistory({ preferSelectLatest: true, force: true })
		return
	}
	const entry = entryFromItem(item)
	const activeId = workspaceSession.activeConnectionId
	if (activeId && entry.connectionId && entry.connectionId !== activeId) {
		return
	}
	const list = historyStore.of(activeId)
	historyStore.patch(activeId, {
		entries: [entry, ...list.entries.filter((h) => h.id !== entry.id)].slice(
			0,
			150,
		),
		error: null,
		selectedId: entry.id,
	})
}

export async function refreshHistory(opts?: {
	preferSelectLatest?: boolean
	force?: boolean
}) {
	const scope = workspaceSession.activeConnectionId ?? null
	const list = historyStore.of(scope)
	if (
		!opts?.force &&
		lastLoaded === scopeKey(scope) &&
		list.loaded &&
		(list.entries.length > 0 || list.error == null)
	) {
		if (opts?.preferSelectLatest && list.entries[0]) {
			historyStore.select(scope, list.entries[0].id)
		}
		return
	}
	if (!isTauri()) {
		historyStore.patch(scope, { entries: [], hasMore: false, loaded: true })
		lastLoaded = scopeKey(scope)
		return
	}
	const gen = nextGen(scope)
	historyStore.patch(scope, { loading: true })
	try {
		const page = await loadHistoryPage(scope, { search: historySearch })
		if (gen !== genOf(scope)) return
		const entries = page.items.map((it) => entryFromItem(it))
		let selectedId = historyStore.of(scope).selectedId
		if (selectedId && !entries.some((h) => h.id === selectedId)) {
			selectedId = null
		}
		if (opts?.preferSelectLatest && entries[0]) selectedId = entries[0].id
		historyStore.patch(scope, {
			entries,
			hasMore: page.hasMore,
			loaded: true,
			error: null,
			selectedId,
		})
		lastLoaded = scopeKey(scope)
	} catch (e) {
		if (gen !== genOf(scope)) return
		const message = errorMessage(e)
		historyStore.patch(scope, { error: message })
		console.warn('[mystique] refreshHistory failed:', message)
	} finally {
		if (gen === genOf(scope)) historyStore.patch(scope, { loading: false })
	}
}

export async function loadOlderHistory() {
	const scope = workspaceSession.activeConnectionId ?? null
	const list = historyStore.of(scope)
	if (list.loadingMore || !list.hasMore) return
	const last = list.entries.at(-1)
	if (!last) return
	const gen = genOf(scope)
	historyStore.patch(scope, { loadingMore: true })
	try {
		const page = await loadHistoryPage(scope, {
			search: historySearch,
			cursorRanAt: last.ranAt,
			cursorId: last.id,
		})
		if (gen !== genOf(scope)) return
		const current = historyStore.of(scope).entries
		const seen = new Set(current.map((row) => row.id))
		const older = page.items
			.map((it) => entryFromItem(it))
			.filter((row) => !seen.has(row.id))
		historyStore.patch(scope, {
			entries: [...current, ...older],
			hasMore: page.hasMore,
		})
	} catch (e) {
		if (gen !== genOf(scope)) return
		historyStore.patch(scope, { error: errorMessage(e) })
	} finally {
		// Always clear it: a refresh in between must not leave "load more" stuck.
		historyStore.patch(scope, { loadingMore: false })
	}
}

async function loadHistoryPage(
	activeId: string | null,
	opts: { search?: string; cursorRanAt?: string; cursorId?: string },
): Promise<QueryHistoryPage> {
	if (activeId) {
		const page = await listQueryHistory(activeId, HISTORY_PAGE, opts)
		if (page.items.length === 0 && !opts.search && !opts.cursorRanAt) {
			const recent = await listRecentQueryHistory(HISTORY_PAGE, opts)
			const items = recent.items.filter((it) => it.connectionId === activeId)
			if (items.length > 0) return { items, hasMore: recent.hasMore }
		}
		return page
	}
	return listRecentQueryHistory(HISTORY_PAGE, opts)
}
