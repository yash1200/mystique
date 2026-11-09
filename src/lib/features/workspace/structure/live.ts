/** Load object structure for the structure tab. */
import type { TableMeta } from '#core/contracts'
import { errorMessage, describeEngineObjectStructure } from '#core/ipc'
import { catalogStore, databaseNameFor } from '#features/workspace/catalog'
import {
	findCatalogTable,
	requireActiveConnection,
	withLiveSession,
	workspaceSession,
	workspaceStudio,
} from '#features/workspace/session'
import { structureStore } from './store.svelte'

export async function openObjectStructure(table: TableMeta, force = false) {
	const active = requireActiveConnection()
	if (!active) return
	const { id } = active
	workspaceStudio.sideMode = 'explorer'
	workspaceSession.openStructure(id, table.id, table.name)
	if (!force && structureStore.get(id, table.id)?.data?.sections?.length) {
		structureStore.setError(id, null)
		return
	}
	structureStore.setLoading(id, true)
	structureStore.setError(id, null)
	try {
		const loaded = await withLiveSession(id, (sid) =>
			describeEngineObjectStructure({
				sessionId: sid,
				table: table.name,
				schema: table.schema || null,
				database: table.database || databaseNameFor(id) || null,
				kind: table.kind,
			}),
		)
		structureStore.put(id, table.id, { data: loaded, rev: Date.now() })
		if (loaded.title) {
			workspaceSession.openStructure(id, table.id, table.name)
		}
	} catch (e) {
		structureStore.setError(id, errorMessage(e))
	} finally {
		structureStore.setLoading(id, false)
	}
}

export function hydrateActiveStructure() {
	const tab = workspaceSession.activeTab
	const id = workspaceSession.activeConnectionId
	if (!id || !tab || tab.kind !== 'structure' || !tab.tableId) return
	const state = structureStore.of(id)
	if (state.loading || state.error) return
	if (structureStore.get(id, tab.tableId)?.data) return
	const table = findCatalogTable(catalogStore.databases(id), tab.tableId)
	if (!table) return
	void openObjectStructure(table)
}
