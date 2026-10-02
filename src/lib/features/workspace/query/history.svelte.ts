/** Query history list. */
import { isTauri } from '#core/ipc'
import { workspaceSession, workspaceStudio } from '#features/workspace/shell'
import {
	historyItemToEntry,
	listQueryHistory,
	listRecentQueryHistory,
} from './history-api'
import type { QueryHistoryItem } from './history-types'

let historyFetchGen = 0

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

export function prependHistoryItem(item: QueryHistoryItem | null | undefined) {
	if (!item?.id) {
		void refreshHistory({ preferSelectLatest: true })
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
		workspaceStudio.historyFor = scope
		return
	}
	const gen = ++historyFetchGen
	workspaceStudio.historyLoading = true
	try {
		let items: QueryHistoryItem[] = []
		if (activeId) {
			items = await listQueryHistory(activeId, 150)
			if (items.length === 0) {
				const recent = await listRecentQueryHistory(150)
				items = recent.filter((it) => it.connectionId === activeId)
			}
		} else {
			items = await listRecentQueryHistory(150)
		}
		if (gen !== historyFetchGen) return
		workspaceStudio.queryHistory = items.map((it) => entryFromItem(it))
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
		workspaceStudio.historyError = e instanceof Error ? e.message : String(e)
		console.warn(
			'[mystique] refreshHistory failed:',
			workspaceStudio.historyError,
		)
	} finally {
		if (gen === historyFetchGen) workspaceStudio.historyLoading = false
	}
}
