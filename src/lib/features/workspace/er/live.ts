/** Load a live ER diagram for a table. */
import type { TableMeta } from '#core/contracts'
import { errorMessage } from '#core/ipc'
import { modelTabTitle, resolveCapabilities } from '#engines'
import { catalogStore, databaseNameFor } from '#features/workspace/catalog'
import {
	findCatalogTable,
	isSessionMissingError,
	reopenEngineSession,
	requireActiveConnection,
	sessionDatabase,
	workspaceSession,
	workspaceStudio,
} from '#features/workspace/session'
import { loadLiveErDiagram } from './api'
import { modelKey, modelStore } from './store.svelte'

export async function openModelDiagram(
	force = false,
	opts?: { database?: string | null; table?: TableMeta | null },
) {
	const active = requireActiveConnection()
	if (!active) return
	const { id, session } = active
	const tab = workspaceSession.activeTab
	const table =
		opts?.table ??
		(tab?.kind === 'model' && tab.tableId ? findTable(id, tab.tableId) : null)
	if (!table) {
		modelStore.setError(id, 'Pick a table to open its ER diagram.')
		return
	}
	workspaceStudio.sideMode = 'explorer'
	const engine = String(session.profile.engine)
	const database = (
		opts?.database ??
		table.database ??
		databaseNameFor(id) ??
		''
	).trim()
	const title = modelTabTitle(
		resolveCapabilities(engine, session.capabilities),
		database,
		table.name,
	)
	const alreadyOpen =
		tab?.kind === 'model' &&
		(tab.modelDatabase ?? '') === database &&
		(tab.tableId ?? '') === table.id
	workspaceSession.openModel(id, database, title, table.id)

	if (
		!force &&
		!alreadyOpen &&
		modelStore.get(id, modelKey(database, table.id))?.diagram?.nodes?.length
	) {
		modelStore.setError(id, null)
		return
	}

	const load = async (sid: string) => {
		const loaded = await loadLiveErDiagram(
			sid,
			engine,
			database || null,
			session.capabilities,
			{ schema: table.schema, name: table.name },
		)
		modelStore.put(id, modelKey(database, table.id), {
			diagram: loaded.diagram,
			title: loaded.title,
			summary: loaded.summary,
			rev: Date.now(),
			focusNodeId: loaded.focusNodeId,
		})
		workspaceSession.openModel(id, database, loaded.title, table.id)
	}

	modelStore.setLoading(id, true)
	modelStore.setError(id, null)
	try {
		let sid = session.engineSessionId
		if (!sid) {
			sid = await reopenEngineSession(
				session.profile,
				sessionDatabase(id, session.profile),
			)
		}
		await load(sid)
	} catch (e) {
		if (isSessionMissingError(e)) {
			try {
				await load(
					await reopenEngineSession(
						session.profile,
						sessionDatabase(id, session.profile),
					),
				)
				modelStore.setError(id, null)
				return
			} catch (retryErr) {
				modelStore.setError(id, errorMessage(retryErr))
				return
			}
		}
		modelStore.setError(id, errorMessage(e))
	} finally {
		modelStore.setLoading(id, false)
	}
}

export function hydrateActiveModel() {
	const active = requireActiveConnection()
	const tab = workspaceSession.activeTab
	if (!active || !tab || tab.kind !== 'model' || !tab.tableId) return
	const state = modelStore.of(active.id)
	if (state.loading || state.error) return
	const db = tab.modelDatabase ?? databaseNameFor(active.id) ?? ''
	if (modelStore.get(active.id, modelKey(db, tab.tableId))?.diagram) return
	const table = findTable(active.id, tab.tableId)
	if (!table) return
	void openModelDiagram(true, { database: db, table })
}

function findTable(connectionId: string, tableId: string): TableMeta | null {
	return findCatalogTable(catalogStore.databases(connectionId), tableId)
}
