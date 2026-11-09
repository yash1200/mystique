/** Import / export a table. Runs go through the background transfer queue. */
import type {
	ImportEncoding,
	ImportFormat,
	PreviewImportResult,
	TableMeta,
} from '#core/contracts'
import {
	cancelEngineQuery,
	errorMessage,
	exportEngineObject,
	importEngineObject,
	previewEngineImport,
} from '#core/ipc'
import { showToast } from '#features/toast'
import { databaseNameFor } from '#features/workspace/catalog'
import {
	dropEditBuffer,
	saveTableTab,
	tabEditsAreDirty,
	tableTabId,
	withLiveSession,
	workspaceSession,
} from '#features/workspace/session'
import { tableSync } from '#features/workspace/table'
import { resolveImportGate, type ImportGateChoice } from './import-gate'
import { TransferQueue, type TransferTask } from './queue.svelte'
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
	/** A query tab on this connection with an open transaction the export can read through. */
	exportTxnTab = $state<{ id: string; title: string } | null>(null)

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

/** Exports and imports running in the background, one at a time per connection. */
export const transferQueue = new TransferQueue({ cancel: (id) => cancelEngineQuery(id) })

function rowsLabel(rows: number): string {
	return `${rows.toLocaleString()} row${rows === 1 ? '' : 's'}`
}

function announce(task: TransferTask) {
	if (task.status === 'done') {
		const what = task.kind === 'export' ? 'Exported' : 'Imported'
		showToast('success', `${what} ${rowsLabel(task.rows)}: ${task.label}`)
	} else if (task.status === 'failed') {
		showToast('danger', `${task.kind === 'export' ? 'Export' : 'Import'} failed: ${task.message ?? ''}`)
	}
}

/** The active tab, or another query tab of this connection, with an open transaction. */
function openTransactionTab(connectionId: string | null): { id: string; title: string } | null {
	if (!connectionId) return null
	const open = (id: string) => !!workspaceSession.tabTxn?.[id]?.inTransaction
	const tabs = workspaceSession.openTabs.filter(
		(t) => t.kind === 'query' && t.connectionId === connectionId && open(t.id),
	)
	const active = tabs.find((t) => t.id === workspaceSession.activeTabId)
	const pick = active ?? tabs[0]
	return pick ? { id: pick.id, title: pick.title } : null
}

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
	transferStore.exportTxnTab = null
}

export function beginExport(table: TableMeta) {
	transferStore.exportConnectionId = workspaceSession.activeConnectionId
	transferStore.exportTable = table
	transferStore.exportError = null
	transferStore.exportTxnTab = openTransactionTab(workspaceSession.activeConnectionId)
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
	tableSync.baselineChanged(tabId)
	const { tablePreview } = await import('#features/workspace/table')
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
	// A new file replaces the old mapping.
	transferStore.importPreview = null
	await loadImportPreview(
		transferStore.importTable,
		spec.path,
		spec.format,
		'utf8',
	)
}

/**
 * Start the import in the background and close the dialog. The queue shows
 * its progress; cancel stops before the next batch (earlier ones are kept).
 */
export async function confirmImportRun(spec: ImportRunSpec) {
	const table = transferStore.importTable
	const connectionId = transferStore.importConnectionId
	const path = transferStore.importPath
	const format = transferStore.importFormat
	if (!table || !connectionId || !path) return
	transferStore.importTable = null
	transferStore.importConnectionId = null
	transferStore.importPreview = null
	transferStore.importPath = ''
	transferStore.importError = null
	const { done } = transferQueue.enqueue(
		{ kind: 'import', connectionId, label: `${fileName(path)} → ${table.name}` },
		async (executionId, onRows) => {
			try {
				// The import dialog is the confirmation, production included.
				const result = await withLiveSession(
					connectionId,
					(sid) =>
						importEngineObject(
							{
								sessionId: sid,
								name: table.name,
								schema: table.schema || null,
								database: table.database || databaseNameFor(connectionId),
								kind: table.kind,
								path,
								format,
								encoding: spec.encoding,
								mapping: spec.mapping,
								onDuplicate: spec.onDuplicate,
								executionId,
							},
							onRows,
						),
					{ replay: false },
				)
				return { rows: result.inserted + result.updated, message: result.message }
			} catch (e) {
				throw new Error(errorMessage(e))
			} finally {
				await reloadImportedTable(connectionId, table)
			}
		},
	)
	announce(await done)
}

/** Import changed the server rows: drop local edits and reload an open tab. */
async function reloadImportedTable(connectionId: string, table: TableMeta) {
	const tabId = tableTabId(connectionId, table.id)
	if (!workspaceSession.openTabs.some((t) => t.id === tabId)) return
	dropEditBuffer(tabId)
	const { tablePreview } = await import('#features/workspace/table')
	void tablePreview.open(table)
}

function fileName(path: string): string {
	return path.split(/[\\/]/).pop() || path
}

/**
 * Queue the export and close the dialog. With `throughTransaction`, the
 * export reads through the query tab's open transaction.
 */
export async function confirmExport(spec: ExportDataSpec) {
	const table = transferStore.exportTable
	const connectionId = transferStore.exportConnectionId
	if (!table || !connectionId) return
	const txnTab = spec.throughTransaction ? transferStore.exportTxnTab : null
	closeExport()
	const { done } = transferQueue.enqueue(
		{
			kind: 'export',
			connectionId,
			label: `${table.name} → ${spec.fileName}`,
			throughTransaction: !!txnTab,
		},
		async (executionId, onRows) => {
			try {
				const { join } = await import('@tauri-apps/api/path')
				const path = await join(spec.folder, spec.fileName)
				const result = await withLiveSession(connectionId, (sid) =>
					exportEngineObject(
						{
							sessionId: sid,
							name: table.name,
							schema: table.schema || null,
							database: table.database || databaseNameFor(connectionId),
							kind: table.kind,
							format: spec.format,
							path,
							executionId,
							tabId: txnTab?.id ?? null,
						},
						onRows,
					),
				)
				return { rows: result.rowCount, message: result.path }
			} catch (e) {
				throw new Error(errorMessage(e))
			}
		},
	)
	announce(await done)
}
