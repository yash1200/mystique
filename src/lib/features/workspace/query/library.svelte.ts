/** Saved-query library list + rename / delete / favorite. */
import { errorMessage, isTauri } from '#core/ipc'
import { workspaceSession, workspaceStudio } from '#features/workspace/session'
import { queryLanguageFor } from './policy'
import {
	deleteSavedQuery,
	listSavedQueries,
	saveQuery,
	toListSavedQuery,
} from './saved-api'

export async function refreshSavedQueries(
	connectionId: string | null,
	force = false,
) {
	if (!connectionId || !isTauri()) {
		workspaceStudio.savedQueries = []
		workspaceStudio.savedQueriesFor = null
		return
	}
	if (!force && workspaceStudio.savedQueriesFor === connectionId) return
	try {
		const rows = await listSavedQueries(connectionId)
		workspaceStudio.savedQueries = rows.map(toListSavedQuery)
		workspaceStudio.savedQueriesFor = connectionId
	} catch (e) {
		console.warn('[mystique] list saved queries:', e)
		workspaceStudio.savedQueries = []
		workspaceStudio.savedQueriesFor = connectionId
	}
}

export function selectSavedQuery(id: string) {
	workspaceStudio.selectedQueryId = id
	const q = workspaceStudio.savedQueries.find((s) => s.id === id)
	const activeId = workspaceSession.activeConnectionId
	if (!q || !activeId) return
	workspaceSession.openSavedQuery(activeId, {
		id: q.id,
		name: q.name,
		body: q.sql,
	})
}

class QueryLibrary {
	renameOpen = $state(false)
	renameId = $state<string | null>(null)
	renameInitial = $state('')
	renameError = $state<string | null>(null)
	renameBusy = $state(false)

	deleteOpen = $state(false)
	deleteId = $state<string | null>(null)
	deleteName = $state('')
	deleteBusy = $state(false)

	async action(
		action: 'duplicate' | 'rename' | 'delete' | 'favorite',
		id: string,
	) {
		const q = workspaceStudio.savedQueries.find((s) => s.id === id)
		const activeId = workspaceSession.activeConnectionId
		if (!q || !activeId) return
		const session = workspaceSession.activeSession
		const language = queryLanguageFor(
			String(session?.profile.engine ?? ''),
			session?.capabilities,
		)

		if (action === 'favorite') {
			try {
				await saveQuery({
					id: q.id,
					connectionId: q.connectionId || activeId,
					name: q.name,
					body: q.sql,
					language,
					favorite: !q.favorite,
				})
				await refreshSavedQueries(activeId, true)
			} catch (e) {
				alert(errorMessage(e))
			}
			return
		}

		if (action === 'duplicate') {
			try {
				const base = q.name.trim() || 'Query'
				const name = base.toLowerCase().endsWith(' copy')
					? `${base} 2`
					: `${base} copy`
				const saved = await saveQuery({
					connectionId: q.connectionId || activeId,
					name,
					body: q.sql,
					language,
				})
				await refreshSavedQueries(activeId, true)
				workspaceStudio.selectedQueryId = saved.id
				workspaceSession.openSavedQuery(activeId, {
					id: saved.id,
					name: saved.name,
					body: saved.body,
				})
			} catch (e) {
				alert(errorMessage(e))
			}
			return
		}

		if (action === 'rename') {
			this.renameId = id
			this.renameInitial = q.name
			this.renameError = null
			this.renameOpen = true
			return
		}

		if (action === 'delete') {
			this.deleteId = id
			this.deleteName = q.name
			this.deleteOpen = true
		}
	}

	async confirmRename(name: string) {
		const id = this.renameId
		const q = id ? workspaceStudio.savedQueries.find((s) => s.id === id) : null
		const activeId = workspaceSession.activeConnectionId
		if (!id || !q || !activeId) {
			this.renameOpen = false
			return
		}
		this.renameBusy = true
		this.renameError = null
		try {
			const session = workspaceSession.activeSession
			const saved = await saveQuery({
				id,
				connectionId: q.connectionId || activeId,
				name,
				body: q.sql,
				language: queryLanguageFor(
					String(session?.profile.engine ?? ''),
					session?.capabilities,
				),
			})
			workspaceSession.renameSavedQueryTabs(id, saved.name)
			await refreshSavedQueries(activeId, true)
			workspaceStudio.selectedQueryId = saved.id
			this.renameOpen = false
			this.renameId = null
		} catch (e) {
			this.renameError = errorMessage(e)
		} finally {
			this.renameBusy = false
		}
	}

	cancelRename() {
		if (this.renameBusy) return
		this.renameOpen = false
		this.renameId = null
		this.renameError = null
	}

	async confirmDelete() {
		const id = this.deleteId
		const activeId = workspaceSession.activeConnectionId
		if (!id || !activeId) {
			this.deleteOpen = false
			return
		}
		this.deleteBusy = true
		try {
			await deleteSavedQuery(id)
			workspaceSession.closeTabsForSavedQuery(id)
			if (workspaceStudio.selectedQueryId === id) {
				workspaceStudio.selectedQueryId = null
			}
			await refreshSavedQueries(activeId, true)
			this.deleteOpen = false
			this.deleteId = null
		} catch (e) {
			alert(errorMessage(e))
		} finally {
			this.deleteBusy = false
		}
	}

	cancelDelete() {
		if (this.deleteBusy) return
		this.deleteOpen = false
		this.deleteId = null
	}
}

export const queryLibrary = new QueryLibrary()
