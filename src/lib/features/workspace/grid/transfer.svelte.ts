/** Import / export a table. Owned by the grid. */
import type { ImportEncoding, ImportFormat, TableMeta } from '#core/contracts'
import {
	exportEngineObject,
	importEngineObject,
	isTauri,
	previewEngineImport,
} from '#core/ipc'
import {
	currentDatabaseName,
	requireActiveConnection,
	tableTabId,
	withLiveSession,
	workspaceSession,
	workspaceStudio,
} from '#features/workspace/shell'
import type { ExportDataSpec } from './ui/ExportDataModal.svelte'
import type { ImportFileSpec } from './ui/ImportDataModal.svelte'
import type { ImportRunSpec } from './ui/ImportPreviewModal.svelte'

export function closeImport() {
	if (workspaceStudio.importBusy) return
	workspaceStudio.importTable = null
	workspaceStudio.importPreview = null
	workspaceStudio.importError = null
	workspaceStudio.importPath = ''
}

export function beginExport(table: TableMeta) {
	workspaceStudio.exportTable = table
	workspaceStudio.exportError = null
}

export function beginImport(table: TableMeta) {
	workspaceStudio.importTable = table
	workspaceStudio.importError = null
	workspaceStudio.importPreview = null
	workspaceStudio.importPath = ''
}

export async function loadImportPreview(
	table: TableMeta,
	path: string,
	format: ImportFormat,
	encoding: ImportEncoding,
) {
	const active = requireActiveConnection()
	if (!active) return
	workspaceStudio.importBusy = true
	workspaceStudio.importError = null
	try {
		workspaceStudio.importPreview = await withLiveSession(active.id, (sid) =>
			previewEngineImport({
				sessionId: sid,
				name: table.name,
				schema: table.schema || null,
				database: table.database || currentDatabaseName(),
				kind: table.kind,
				path,
				format,
				encoding,
			}),
		)
		workspaceStudio.importPath = path
		workspaceStudio.importFormat = format
	} catch (e) {
		workspaceStudio.importError = e instanceof Error ? e.message : String(e)
	} finally {
		workspaceStudio.importBusy = false
	}
}

export async function confirmImportFile(spec: ImportFileSpec) {
	if (!workspaceStudio.importTable) return
	if (!isTauri()) {
		workspaceStudio.importError = 'Import requires the desktop app.'
		return
	}
	await loadImportPreview(
		workspaceStudio.importTable,
		spec.path,
		spec.format,
		'utf8',
	)
}

export async function confirmImportRun(spec: ImportRunSpec) {
	const table = workspaceStudio.importTable
	const active = requireActiveConnection()
	if (!table || !active || !workspaceStudio.importPath) return
	workspaceStudio.importBusy = true
	workspaceStudio.importError = null
	try {
		await withLiveSession(active.id, (sid) =>
			importEngineObject({
				sessionId: sid,
				name: table.name,
				schema: table.schema || null,
				database: table.database || currentDatabaseName(),
				kind: table.kind,
				path: workspaceStudio.importPath,
				format: workspaceStudio.importFormat,
				encoding: spec.encoding,
				mapping: spec.mapping,
				onDuplicate: spec.onDuplicate,
				profileReadOnly: !!active.session.profile.readOnly,
			}),
		)
		workspaceStudio.importTable = null
		workspaceStudio.importPreview = null
		workspaceStudio.importPath = ''
		const tabId = tableTabId(active.id, table.id)
		if (workspaceSession.openTabs.some((t) => t.id === tabId)) {
			const { tablePreview } = await import('./tab-preview.svelte')
			void tablePreview.open(table)
		}
	} catch (e) {
		workspaceStudio.importError = e instanceof Error ? e.message : String(e)
	} finally {
		workspaceStudio.importBusy = false
	}
}

export async function confirmExport(spec: ExportDataSpec) {
	const table = workspaceStudio.exportTable
	const active = requireActiveConnection()
	if (!table || !active) return
	if (!isTauri()) {
		workspaceStudio.exportError = 'Export requires the desktop app.'
		return
	}
	workspaceStudio.exportBusy = true
	workspaceStudio.exportError = null
	try {
		const { join } = await import('@tauri-apps/api/path')
		const path = await join(spec.folder, spec.fileName)
		await withLiveSession(active.id, (sid) =>
			exportEngineObject({
				sessionId: sid,
				name: table.name,
				schema: table.schema || null,
				database: table.database || currentDatabaseName(),
				kind: table.kind,
				format: spec.format,
				path,
			}),
		)
		workspaceStudio.exportTable = null
	} catch (e) {
		workspaceStudio.exportError = e instanceof Error ? e.message : String(e)
	} finally {
		workspaceStudio.exportBusy = false
	}
}
