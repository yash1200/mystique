/** Explorer context-menu actions. */
import type { TableAdminOp, TableMeta } from '#core/contracts'
import { applyEngineTableAdmin, isTauri } from '#core/ipc'
import { refreshCurrentTables } from '#features/workspace/catalog'
import { openModelDiagram } from '#features/workspace/er'
import {
	beginExport,
	beginImport,
	tablePreview,
} from '#features/workspace/grid'
import { openSqlInEditor } from '#features/workspace/query'
import {
	currentDatabaseName,
	requireActiveConnection,
	structureCacheKey,
	tableTabId,
	runWithWriteGuard,
	withLiveSession,
	workspaceSession,
	workspaceStudio,
} from '#features/workspace/shell'
import { openObjectStructure } from './structure-live'
import type { TableExplorerAction } from './ui/ObjectExplorer.svelte'

export async function onTableExplorerAction(
	action: TableExplorerAction,
	table: TableMeta,
	extra?: string,
) {
	const active = requireActiveConnection()
	if (!active) return
	const { id, session } = active

	if (action === 'open') {
		void tablePreview.open(table)
		return
	}
	if (action === 'erd') {
		void openModelDiagram(false, { table })
		return
	}
	if (action === 'structure') {
		void openObjectStructure(table)
		return
	}
	if (action === 'export') {
		beginExport(table)
		return
	}
	if (action === 'import') {
		beginImport(table)
		return
	}
	if (!isTauri()) {
		workspaceStudio.setCatalogError(
			'Live catalog actions require the desktop app.',
		)
		return
	}

	const newName = extra?.trim() || null
	if ((action === 'rename' || action === 'duplicate') && !newName) return

	const op: TableAdminOp = action === 'create' ? 'createStatement' : action

	try {
		workspaceStudio.setCatalogError(null)
		const result = await withLiveSession(
			id,
			(sid) =>
				runWithWriteGuard(sid, (confirmObjectName) =>
					applyEngineTableAdmin({
						sessionId: sid,
						op,
						name: table.name,
						schema: table.schema || null,
						database: table.database || currentDatabaseName(),
						kind: table.kind,
						newName,
						profileReadOnly: !!session.profile.readOnly,
						confirmObjectName: confirmObjectName ?? null,
					}),
				),
			{ replay: false },
		)

		if (action === 'create') {
			const sql = (result.statement ?? '').trim()
			if (!sql) {
				workspaceStudio.setCatalogError(
					'Engine returned an empty create statement.',
				)
				return
			}
			openSqlInEditor(
				sql.endsWith(';') || sql.includes('createCollection')
					? `${sql}\n`
					: `${sql};\n`,
				`${table.name} · create`,
			)
			return
		}

		const tabId = tableTabId(id, table.id)
		const structureTabId = `${id}::structure::${table.id}`
		const tableOpen = workspaceSession.openTabs.some((t) => t.id === tabId)
		const structureOpen = workspaceSession.openTabs.some(
			(t) => t.id === structureTabId,
		)
		if (action === 'drop' || action === 'rename') {
			if (tableOpen) workspaceSession.closeTab(tabId)
			if (structureOpen) workspaceSession.closeTab(structureTabId)
			const sk = structureCacheKey(id, table.id)
			if (workspaceStudio.liveStructures[sk]) {
				const { [sk]: _drop, ...rest } = workspaceStudio.liveStructures
				workspaceStudio.liveStructures = rest
			}
		} else if (action === 'truncate' && tableOpen) {
			void tablePreview.open(table)
		}
		await refreshCurrentTables()
	} catch (e) {
		workspaceStudio.setCatalogError(
			e instanceof Error ? e.message : String(e),
		)
	}
}
