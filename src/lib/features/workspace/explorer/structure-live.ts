/** Load object structure for the structure tab. */
import type { TableMeta } from '$core/contracts'
import { describeEngineObjectStructure, isTauri } from '$core/ipc'
import {
	currentDatabaseName,
	requireActiveConnection,
	structureCacheKey,
	withLiveSession,
	workspaceSession,
	workspaceStudio,
} from '$features/workspace/shell'

export async function openObjectStructure(table: TableMeta, force = false) {
	const active = requireActiveConnection()
	if (!active) return
	const { id } = active
	workspaceStudio.sideMode = 'explorer'
	workspaceSession.openStructure(id, table.id, table.name)
	const key = structureCacheKey(id, table.id)
	if (!force && workspaceStudio.liveStructures[key]?.data?.sections?.length) {
		workspaceStudio.structureError = null
		return
	}
	if (!isTauri()) {
		workspaceStudio.structureError =
			'Object structure requires Tauri. Run `npm run tauri dev` and Connect.'
		return
	}
	workspaceStudio.structureLoading = true
	workspaceStudio.structureError = null
	try {
		const loaded = await withLiveSession(id, (sid) =>
			describeEngineObjectStructure({
				sessionId: sid,
				table: table.name,
				schema: table.schema || null,
				database: table.database || currentDatabaseName() || null,
				kind: table.kind,
			}),
		)
		workspaceStudio.liveStructures = {
			...workspaceStudio.liveStructures,
			[key]: { data: loaded, rev: Date.now() },
		}
		if (loaded.title) {
			workspaceSession.openStructure(id, table.id, table.name)
		}
	} catch (e) {
		workspaceStudio.structureError = e instanceof Error ? e.message : String(e)
	} finally {
		workspaceStudio.structureLoading = false
	}
}

export function hydrateActiveStructure() {
	const tab = workspaceSession.activeTab
	const id = workspaceSession.activeConnectionId
	if (
		!id ||
		!tab ||
		tab.kind !== 'structure' ||
		!tab.tableId ||
		workspaceStudio.structureLoading ||
		workspaceStudio.structureError
	) {
		return
	}
	const key = structureCacheKey(id, tab.tableId)
	if (workspaceStudio.liveStructures[key]?.data) return
	const table = findStructureTable(id, tab.tableId)
	if (!table) return
	void openObjectStructure(table)
}

function findStructureTable(
	connectionId: string,
	tableId: string,
): TableMeta | null {
	const cat = workspaceStudio.catalogs[connectionId] ?? []
	for (const d of cat) {
		const t = d.tables.find((x) => x.id === tableId)
		if (t) return t
	}
	return null
}
