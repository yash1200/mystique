/** Import / export a table. */
import type {
	ImportEncoding,
	ImportFormat,
	PreviewImportResult,
	TableMeta,
} from '#core/contracts'
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
import { databaseNameFor } from '#features/workspace/catalog'
import {
	dropEditBuffer,
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

/**
 * The import and export dialogs. One of each is open at a time (they are
 * modal), and each is bound to the connection it was opened on: every call it
 * makes goes to that connection, whichever one is active by then.
 */
class TransferStore {
	exportConnectionId = $state<string | null>(null)
	exportTable = $state<TableMeta | null>(null)
	exportBusy = $state(false)
	exportError = $state<string | null>(null)

	importConnectionId = $state<string | null>(null)
	importTable = $state<TableMeta | null>(null)
	importBusy = $state(false)
	importError = $state<string | null>(null)
	importPath = $state('')
	importPreview = $state<PreviewImportResult | null>(null)
	importFormat = $state<ImportFormat>('csv')

	/** The connection the open dialog belongs to. */
	get connectionId(): string | null {
		if (this.importTable) return this.importConnectionId
		if (this.exportTable) return this.exportConnectionId
		return null
	}
}

export const transferStore = new TransferStore()

export function closeImport() {
	if (transferStore.importBusy) return
	transferStore.importTable = null
	transferStore.importConnectionId = null
	transferStore.importPreview = null
	transferStore.importError = null
	transferStore.importPath = ''
}

export function closeExport() {
	transferStore.exportTable = null
	transferStore.exportConnectionId = null
	transferStore.exportError = null
}

export function beginExport(table: TableMeta) {
	transferStore.exportConnectionId = workspaceSession.activeConnectionId
	transferStore.exportTable = table
	transferStore.exportError = null
}

/** Set while import asks about the table's pending grid edits. */
export const importGate = $state({
	table: null as TableMeta | null,
	connectionId: null as string | null,
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
	const connectionId = workspaceSession.activeConnectionId
	const tabId = connectionId ? tableTabId(connectionId, table.id) : ''
	if (tabId && tabHasEdits(tabId)) {
		importGate.table = table
		importGate.connectionId = connectionId
		importGate.tabId = tabId
		importGate.error = null
		return
	}
	openImport(table, connectionId)
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
	openImport(table, importGate.connectionId)
}

async function discardTabEdits(table: TableMeta, tabId: string) {
	dropEditBuffer(tabId)
	workspaceSession.setTabDirty(tabId, false)
	workspaceStudio.baselineEpoch += 1
	const { tablePreview } = await import('#features/workspace/grid')
	await tablePreview.open(table)
}

function openImport(table: TableMeta, connectionId: string | null) {
	transferStore.importConnectionId = connectionId
	transferStore.importTable = table
	transferStore.importError = null
	transferStore.importPreview = null
	transferStore.importPath = ''
}

export async function loadImportPreview(
	table: TableMeta,
	path: string,
	format: ImportFormat,
	encoding: ImportEncoding,
) {
	const connectionId = transferStore.importConnectionId
	if (!connectionId) return
	transferStore.importBusy = true
	transferStore.importError = null
	try {
		transferStore.importPreview = await withLiveSession(connectionId, (sid) =>
			previewEngineImport({
				sessionId: sid,
				name: table.name,
				schema: table.schema || null,
				database: table.database || databaseNameFor(connectionId),
				kind: table.kind,
				path,
				format,
				encoding,
			}),
		)
		transferStore.importPath = path
		transferStore.importFormat = format
	} catch (e) {
		transferStore.importError = errorMessage(e)
	} finally {
		transferStore.importBusy = false
	}
}

export async function confirmImportFile(spec: ImportFileSpec) {
	if (!transferStore.importTable) return
	if (!isTauri()) {
		transferStore.importError = 'Import requires the desktop app.'
		return
	}
	// A new file replaces the old mapping.
	transferStore.importPreview = null
	await loadImportPreview(
		transferStore.importTable,
		spec.path,
		spec.format,
		'utf8',
	)
}

export async function confirmImportRun(spec: ImportRunSpec) {
	const table = transferStore.importTable
	const connectionId = transferStore.importConnectionId
	if (!table || !connectionId || !transferStore.importPath) return
	transferStore.importBusy = true
	transferStore.importError = null
	try {
		// The import dialog is the confirmation, production included.
		await withLiveSession(
			connectionId,
			(sid) =>
				importEngineObject({
					sessionId: sid,
					name: table.name,
					schema: table.schema || null,
					database: table.database || databaseNameFor(connectionId),
					kind: table.kind,
					path: transferStore.importPath,
					format: transferStore.importFormat,
					encoding: spec.encoding,
					mapping: spec.mapping,
					onDuplicate: spec.onDuplicate,
				}),
			{ replay: false },
		)
		transferStore.importTable = null
		transferStore.importConnectionId = null
		transferStore.importPreview = null
		transferStore.importPath = ''
		const tabId = tableTabId(connectionId, table.id)
		if (workspaceSession.openTabs.some((t) => t.id === tabId)) {
			// Import already changed the server page. Drop local edits so the
			// reload is allowed to replace them.
			dropEditBuffer(tabId)
			const { tablePreview } = await import('#features/workspace/grid')
			void tablePreview.open(table)
		}
	} catch (e) {
		transferStore.importError = errorMessage(e)
	} finally {
		transferStore.importBusy = false
	}
}

let exportRunTab: string | null = null

export function cancelActiveExport() {
	if (!exportRunTab) return
	void cancelTabRun(exportRunTab, 'export')
}

export async function confirmExport(spec: ExportDataSpec) {
	const table = transferStore.exportTable
	const connectionId = transferStore.exportConnectionId
	if (!table || !connectionId) return
	if (!isTauri()) {
		transferStore.exportError = 'Export requires the desktop app.'
		return
	}
	transferStore.exportBusy = true
	transferStore.exportError = null
	const tabId = tableTabId(connectionId, table.id)
	exportRunTab = tabId
	const executionId = startTabRun(tabId, 'export')
	try {
		const { join } = await import('@tauri-apps/api/path')
		const path = await join(spec.folder, spec.fileName)
		await withLiveSession(connectionId, (sid) =>
			exportEngineObject({
				sessionId: sid,
				name: table.name,
				schema: table.schema || null,
				database: table.database || databaseNameFor(connectionId),
				kind: table.kind,
				format: spec.format,
				path,
				executionId,
			}),
		)
		closeExport()
	} catch (e) {
		transferStore.exportError = errorMessage(e)
	} finally {
		finishTabRun(tabId, executionId)
		if (exportRunTab === tabId) exportRunTab = null
		transferStore.exportBusy = false
	}
}
