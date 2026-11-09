/** Explorer context-menu actions. */
import type { TableAdminOp, TableMeta } from '#core/contracts'
import { errorMessage, applyEngineTableAdmin } from '#core/ipc'
import {
	catalogStore,
	databaseNameFor,
	refreshCurrentTables,
} from '#features/workspace/catalog'
import { openModelDiagram } from '#features/workspace/er'
import { tablePreview } from '#features/workspace/table'
import { openSqlInEditor } from '#features/workspace/query'
import {
	askWriteGuard,
	isWriteCancelled,
	requireActiveConnection,
	tableTabId,
	runWithWriteGuard,
	withLiveSession,
	workspaceSession,
} from '#features/workspace/session'
import {
	openObjectStructure,
	structureStore,
} from '#features/workspace/structure'
import { beginExport, beginImport } from '#features/workspace/transfer'
import { menuDangerPrompt } from './danger'
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

	const newName = extra?.trim() || null
	if ((action === 'rename' || action === 'duplicate') && !newName) return

	const op: TableAdminOp = action === 'create' ? 'createStatement' : action

	// Drop / truncate: one dialog. Off production it is this one, and the run
	// carries the name so the backend does not ask again.
	let confirmed: string | undefined
	if (action === 'drop' || action === 'truncate') {
		const prompt = menuDangerPrompt({
			action,
			name: table.name,
			noun: table.kind || 'table',
			environment: session.profile.environment ?? 'local',
			connectionName: session.profile.name,
		})
		if (prompt) {
			const decision = await askWriteGuard(prompt)
			if (!decision.ok) return
			confirmed = table.name
		}
	}

	try {
		catalogStore.setError(id, null)
		const result = await withLiveSession(
			id,
			(sid) =>
				runWithWriteGuard(
					sid,
					(confirmObjectName) =>
						applyEngineTableAdmin({
							sessionId: sid,
							op,
							name: table.name,
							schema: table.schema || null,
							database: table.database || databaseNameFor(id),
							kind: table.kind,
							newName,
							confirmObjectName: confirmObjectName ?? null,
						}),
					{ confirmObjectName: confirmed },
				),
			{ replay: false },
		)

		if (action === 'create') {
			const sql = (result.statement ?? '').trim()
			if (!sql) {
				catalogStore.setError(id, 'Engine returned an empty create statement.')
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
			structureStore.drop(id, table.id)
		} else if (action === 'truncate' && tableOpen) {
			void tablePreview.open(table)
		}
		await refreshCurrentTables()
	} catch (e) {
		if (isWriteCancelled(e)) return
		catalogStore.setError(id, errorMessage(e))
	}
}
