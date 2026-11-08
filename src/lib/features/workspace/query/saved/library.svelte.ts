/** Saved-query library list + rename / delete / favorite. */
import { errorMessage, isTauri } from '#core/ipc'
import { workspaceSession } from '#features/workspace/session'
import { queryLanguageFor } from '../policy'
import {
	deleteSavedQuery,
	listSavedQueries,
	saveQuery,
	toListSavedQuery,
} from './saved-api'
import type { SavedQueryListItem } from './saved-types'

/** One connection's saved queries. */
export type SavedList = {
	items: SavedQueryListItem[]
	selectedId: string | null
}

const EMPTY: SavedList = Object.freeze({
	items: Object.freeze([]) as unknown as SavedQueryListItem[],
	selectedId: null,
})

/** Saved-query lists keyed by connectionId. */
class SavedQueriesStore {
	lists = $state<Record<string, SavedList>>({})

	of(connectionId: string | null | undefined): SavedList {
		return (connectionId && this.lists[connectionId]) || EMPTY
	}

	/** The active connection's list. */
	get current(): SavedList {
		return this.of(workspaceSession.activeConnectionId)
	}

	patch(connectionId: string, next: Partial<SavedList>) {
		this.lists = {
			...this.lists,
			[connectionId]: { ...this.of(connectionId), ...next },
		}
	}

	select(connectionId: string | null | undefined, id: string | null) {
		if (!connectionId) return
		this.patch(connectionId, { selectedId: id })
	}

	forget(connectionId: string) {
		if (!(connectionId in this.lists)) return
		const { [connectionId]: _drop, ...rest } = this.lists
		this.lists = rest
	}
}

export const savedStore = new SavedQueriesStore()

/** Fetch generation per connection: a newer fetch drops an older one's rows. */
const fetchGen = new Map<string, number>()
/** The list loaded last. Switching connections refetches, as it always has. */
let lastLoaded: string | null = null

export async function refreshSavedQueries(
	connectionId: string | null,
	force = false,
) {
	if (!connectionId || !isTauri()) {
		lastLoaded = null
		return
	}
	if (!force && lastLoaded === connectionId) return
	const gen = (fetchGen.get(connectionId) ?? 0) + 1
	fetchGen.set(connectionId, gen)
	let items: SavedQueryListItem[]
	try {
		items = (await listSavedQueries(connectionId)).map(toListSavedQuery)
	} catch (e) {
		console.warn('[mystique] list saved queries:', e)
		items = []
	}
	// A late answer for an older fetch must not replace a newer list.
	if (fetchGen.get(connectionId) !== gen) return
	savedStore.patch(connectionId, { items })
	lastLoaded = connectionId
}

export function selectSavedQuery(id: string) {
	const activeId = workspaceSession.activeConnectionId
	savedStore.select(activeId, id)
	const q = savedStore.of(activeId).items.find((s) => s.id === id)
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
		const activeId = workspaceSession.activeConnectionId
		const q = savedStore.of(activeId).items.find((s) => s.id === id)
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
				savedStore.select(activeId, saved.id)
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
		const activeId = workspaceSession.activeConnectionId
		const q = id
			? savedStore.of(activeId).items.find((s) => s.id === id)
			: null
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
			savedStore.select(activeId, saved.id)
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
			if (savedStore.of(activeId).selectedId === id) {
				savedStore.select(activeId, null)
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
