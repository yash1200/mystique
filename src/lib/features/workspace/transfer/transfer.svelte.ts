/** Import / export a table. */
import type { ImportEncoding, ImportFormat, TableMeta } from '#core/contracts'
import {
	errorMessage,
	cancelTabRun,
	exportEngineObject,
	finishTabRun,
	importEngineObject,
	isTauri,
	previewEngineImport,
	startTabRun,
} from '#core/ipc'
import {
	currentDatabaseName,
	dropEditBuffer,
	requireActiveConnection,
	saveTableTab,
	tabEditsAreDirty,
	tableTabId,
	withLiveSession,
	workspaceSession,
	workspaceStudio,
} from '#features/workspace/session'
import { resolveImportGate, type ImportGateChoice } from './import-gate'
import type { ExportDataSpec } from './ui/ExportDataModal.svelte'
import type { ImportFileSpec, ImportRunSpec } from './ui/ImportDataModal.svelte'

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

/** Set while import asks about the table's pending grid edits. */
export const importGate = $state({
	table: null as TableMeta | null,
	tabId: '',
	/** The grid's save dialogs are up; the gate hides meanwhile. */
	saving: false,
	error: null as string | null,
})

function tabHasEdits(tabId: string): boolean {
	const tab = workspaceSession.openTabs.find((t) => t.id === tabId)
	return !!tab && (tab.dirty || tabEditsAreDirty(tabId))
}

export function beginImport(table: TableMeta) {
	const active = requireActiveConnection()
	const tabId = active ? tableTabId(active.id, table.id) : ''
	if (tabId && tabHasEdits(tabId)) {
		importGate.table = table
		importGate.tabId = tabId
		importGate.error = null
		return
	}
	openImport(table)
}

export async function answerImportGate(choice: ImportGateChoice) {
	const table = importGate.table
	const tabId = importGate.tabId
	if (!table || importGate.saving) return
	importGate.error = null
	if (choice === 'save') {
		// Only the active tab's grid is mounted, and it owns the save.
		workspaceSession.setActiveTab(tabId)
		importGate.saving = true
	}
	const result = await resolveImportGate(choice, {
		save: () => saveTableTab(tabId),
		discard: () => discardTabEdits(table, tabId),
	})
	importGate.saving = false
	if (!result.proceed) {
		if (choice === 'cancel') importGate.table = null
		else importGate.error = result.error
		return
	}
	importGate.table = null
	openImport(table)
}

async function discardTabEdits(table: TableMeta, tabId: string) {
	dropEditBuffer(tabId)
	workspaceSession.setTabDirty(tabId, false)
	workspaceStudio.baselineEpoch += 1
	const { tablePreview } = await import('#features/workspace/grid')
	await tablePreview.open(table)
}

function openImport(table: TableMeta) {
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
		workspaceStudio.importError = errorMessage(e)
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
	// A new file replaces the old mapping.
	workspaceStudio.importPreview = null
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
		// The import dialog is the confirmation, production included.
		await withLiveSession(
			active.id,
			(sid) =>
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
				}),
			{ replay: false },
		)
		workspaceStudio.importTable = null
		workspaceStudio.importPreview = null
		workspaceStudio.importPath = ''
		const tabId = tableTabId(active.id, table.id)
		if (workspaceSession.openTabs.some((t) => t.id === tabId)) {
			// Import already changed the server page. Drop local edits so the
			// reload is allowed to replace them.
			dropEditBuffer(tabId)
			const { tablePreview } = await import('#features/workspace/grid')
			void tablePreview.open(table)
		}
	} catch (e) {
		workspaceStudio.importError = errorMessage(e)
	} finally {
		workspaceStudio.importBusy = false
	}
}

let exportRunTab: string | null = null

export function cancelActiveExport() {
	if (!exportRunTab) return
	void cancelTabRun(exportRunTab, 'export')
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
	const tabId = tableTabId(active.id, table.id)
	exportRunTab = tabId
	const executionId = startTabRun(tabId, 'export')
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
				executionId,
			}),
		)
		workspaceStudio.exportTable = null
	} catch (e) {
		workspaceStudio.exportError = errorMessage(e)
	} finally {
		finishTabRun(tabId, executionId)
		if (exportRunTab === tabId) exportRunTab = null
		workspaceStudio.exportBusy = false
	}
}
