/** Load a live ER diagram for a table. */
import type { TableMeta } from '#core/contracts'
import { isTauri } from '#core/ipc'
import { modelTabTitle, resolveCapabilities } from '#engines'
import {
	currentDatabaseName,
	isSessionMissingError,
	modelCacheKey,
	reopenEngineSession,
	requireActiveConnection,
	sessionDatabase,
	workspaceSession,
	workspaceStudio,
} from '#features/workspace/shell'
import { loadLiveErDiagram } from './api'

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
		workspaceStudio.modelError = 'Pick a table to open its ER diagram.'
		return
	}
	workspaceStudio.sideMode = 'explorer'
	const engine = String(session.profile.engine)
	const database = (
		opts?.database ??
		table.database ??
		currentDatabaseName() ??
		''
	).trim()
	const title = modelTabTitle(
		resolveCapabilities(engine, session.capabilities),
		database,
		table.name,
	)
	const key = modelCacheKey(id, database, table.id)
	const alreadyOpen =
		tab?.kind === 'model' &&
		(tab.modelDatabase ?? '') === database &&
		(tab.tableId ?? '') === table.id
	workspaceSession.openModel(id, database, title, table.id)

	if (
		!force &&
		!alreadyOpen &&
		workspaceStudio.liveModels[key]?.diagram?.nodes?.length
	) {
		workspaceStudio.modelError = null
		return
	}

	if (!isTauri()) {
		workspaceStudio.modelError =
			'Live ER diagram requires the Mystique desktop app. Connect and try again.'
		return
	}

	workspaceStudio.modelLoading = true
	workspaceStudio.modelError = null
	try {
		let sid = session.engineSessionId
		if (!sid) {
			sid = await reopenEngineSession(
				session.profile,
				sessionDatabase(id, session.profile),
			)
		}
		const loaded = await loadLiveErDiagram(
			sid,
			engine,
			database || null,
			session.capabilities,
			{ schema: table.schema, name: table.name },
		)
		workspaceStudio.liveModels = {
			...workspaceStudio.liveModels,
			[key]: {
				diagram: loaded.diagram,
				title: loaded.title,
				summary: loaded.summary,
				rev: Date.now(),
				focusNodeId: loaded.focusNodeId,
			},
		}
		workspaceSession.openModel(id, database, loaded.title, table.id)
	} catch (e) {
		if (isSessionMissingError(e)) {
			try {
				const sid = await reopenEngineSession(
					session.profile,
					sessionDatabase(id, session.profile),
				)
				const loaded = await loadLiveErDiagram(
					sid,
					engine,
					database || null,
					session.capabilities,
					{ schema: table.schema, name: table.name },
				)
				workspaceStudio.liveModels = {
					...workspaceStudio.liveModels,
					[key]: {
						diagram: loaded.diagram,
						title: loaded.title,
						summary: loaded.summary,
						rev: Date.now(),
						focusNodeId: loaded.focusNodeId,
					},
				}
				workspaceSession.openModel(id, database, loaded.title, table.id)
				workspaceStudio.modelError = null
				return
			} catch (retryErr) {
				workspaceStudio.modelError =
					retryErr instanceof Error ? retryErr.message : String(retryErr)
				return
			}
		}
		workspaceStudio.modelError = e instanceof Error ? e.message : String(e)
	} finally {
		workspaceStudio.modelLoading = false
	}
}

export function hydrateActiveModel() {
	const active = requireActiveConnection()
	const tab = workspaceSession.activeTab
	if (
		!active ||
		!tab ||
		tab.kind !== 'model' ||
		!tab.tableId ||
		workspaceStudio.modelLoading ||
		workspaceStudio.modelError
	) {
		return
	}
	const db = tab.modelDatabase ?? currentDatabaseName() ?? ''
	if (
		workspaceStudio.liveModels[modelCacheKey(active.id, db, tab.tableId)]
			?.diagram
	) {
		return
	}
	const table = findTable(active.id, tab.tableId)
	if (!table) return
	void openModelDiagram(true, { database: db, table })
}

function findTable(connectionId: string, tableId: string): TableMeta | null {
	const cat = workspaceStudio.catalogs[connectionId] ?? []
	for (const d of cat) {
		const t = d.tables.find((x) => x.id === tableId)
		if (t) return t
	}
	return null
}
