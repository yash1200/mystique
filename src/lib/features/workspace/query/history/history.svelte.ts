/** Query history list. The connection manager writes the rows. */
import { errorMessage, isTauri } from '#core/ipc'
import { workspaceSession, workspaceStudio } from '#features/workspace/session'
import {
	historyItemToEntry,
	listQueryHistory,
	listRecentQueryHistory,
	type QueryHistoryPage,
} from './history-api'
import type { QueryHistoryItem } from './history-types'

/** One keyset page. The server keeps 2_000 runs per connection. */
const HISTORY_PAGE = 100

let historyFetchGen = 0
let historySearch = ''

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
	workspaceStudio.selectedHistoryId = id
	const h = workspaceStudio.queryHistory.find((x) => x.id === id)
	if (!h || !workspaceSession.activeConnectionId) return
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
	workspaceStudio.queryHistory = [
		entry,
		...workspaceStudio.queryHistory.filter((h) => h.id !== entry.id),
	].slice(0, 150)
	workspaceStudio.historyError = null
	workspaceStudio.selectedHistoryId = entry.id
}

export async function refreshHistory(opts?: {
	preferSelectLatest?: boolean
	force?: boolean
}) {
	const activeId = workspaceSession.activeConnectionId
	const scope = activeId ?? null
	if (
		!opts?.force &&
		workspaceStudio.historyFor === scope &&
		(workspaceStudio.queryHistory.length > 0 ||
			workspaceStudio.historyError == null)
	) {
		if (opts?.preferSelectLatest && workspaceStudio.queryHistory[0]) {
			workspaceStudio.selectedHistoryId = workspaceStudio.queryHistory[0].id
		}
		return
	}
	if (!isTauri()) {
		workspaceStudio.queryHistory = []
		workspaceStudio.historyHasMore = false
		workspaceStudio.historyFor = scope
		return
	}
	const gen = ++historyFetchGen
	workspaceStudio.historyLoading = true
	try {
		const page = await loadHistoryPage(activeId, {
			search: historySearch,
		})
		if (gen !== historyFetchGen) return
		workspaceStudio.queryHistory = page.items.map((it) => entryFromItem(it))
		workspaceStudio.historyHasMore = page.hasMore
		workspaceStudio.historyFor = scope
		workspaceStudio.historyError = null
		if (
			workspaceStudio.selectedHistoryId &&
			!workspaceStudio.queryHistory.some(
				(h) => h.id === workspaceStudio.selectedHistoryId,
			)
		) {
			workspaceStudio.selectedHistoryId = null
		}
		if (opts?.preferSelectLatest && workspaceStudio.queryHistory[0]) {
			workspaceStudio.selectedHistoryId = workspaceStudio.queryHistory[0].id
		}
	} catch (e) {
		if (gen !== historyFetchGen) return
		workspaceStudio.historyError = errorMessage(e)
		console.warn(
			'[mystique] refreshHistory failed:',
			workspaceStudio.historyError,
		)
	} finally {
		if (gen === historyFetchGen) workspaceStudio.historyLoading = false
	}
}

export async function loadOlderHistory() {
	if (workspaceStudio.historyLoadingMore || !workspaceStudio.historyHasMore) return
	const activeId = workspaceSession.activeConnectionId
	const last = workspaceStudio.queryHistory.at(-1)
	if (!last) return
	const gen = historyFetchGen
	workspaceStudio.historyLoadingMore = true
	try {
		const page = await loadHistoryPage(activeId, {
			search: historySearch,
			cursorRanAt: last.ranAt,
			cursorId: last.id,
		})
		if (gen !== historyFetchGen) return
		const seen = new Set(workspaceStudio.queryHistory.map((row) => row.id))
		const older = page.items
			.map((it) => entryFromItem(it))
			.filter((row) => !seen.has(row.id))
		workspaceStudio.queryHistory = [...workspaceStudio.queryHistory, ...older]
		workspaceStudio.historyHasMore = page.hasMore
	} catch (e) {
		if (gen !== historyFetchGen) return
		workspaceStudio.historyError = errorMessage(e)
	} finally {
		if (gen === historyFetchGen) workspaceStudio.historyLoadingMore = false
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
