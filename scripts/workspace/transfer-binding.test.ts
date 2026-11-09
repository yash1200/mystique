import { beforeEach, expect, test, vi } from 'vitest'

// D-06: an import or export dialog is bound to the connection it was opened
// on. Switching the active connection while it is open must not send the run
// to the other connection.
const session = {
	activeConnectionId: 'a' as string | null,
	activeTabId: null as string | null,
	openTabs: [] as {
		id: string
		dirty?: boolean
		kind?: string
		connectionId?: string
		title?: string
	}[],
	tabTxn: {} as Record<string, { pinned: boolean; inTransaction: boolean }>,
}
const liveCalls: string[] = []
const requests: { database: string | null; kind: string; tabId?: string | null }[] = []

vi.mock('#core/ipc', () => ({
	errorMessage: (e: unknown) => String(e),
	startTabRun: () => 'exec-1',
	finishTabRun: () => {},
	cancelTabRun: async () => {},
	cancelEngineQuery: async () => {},
	previewEngineImport: async (req: { database: string | null }) => {
		requests.push({ database: req.database, kind: 'preview' })
		return { columns: [], rows: [] }
	},
	importEngineObject: async (req: { database: string | null }) => {
		requests.push({ database: req.database, kind: 'import' })
		return { inserted: 1, updated: 0, format: 'csv' }
	},
	exportEngineObject: async (req: { database: string | null; tabId?: string | null }) => {
		requests.push({ database: req.database, kind: 'export', tabId: req.tabId })
		return { path: '/tmp/x', rowCount: 3, format: 'csv' }
	},
}))
vi.mock('@tauri-apps/api/path', () => ({
	join: async (...parts: string[]) => parts.join('/'),
}))
vi.mock('#features/workspace/catalog', () => ({
	databaseNameFor: (id: string) => `db-of-${id}`,
}))
vi.mock('#features/workspace/session', () => ({
	workspaceSession: session,
	workspaceStudio: { baselineEpoch: 0 },
	dropEditBuffer: () => {},
	saveTableTab: async () => ({ ok: true, error: null }),
	tabEditsAreDirty: () => false,
	tableTabId: (c: string, t: string) => `${c}::table::${t}`,
	withLiveSession: async (id: string, fn: (sid: string) => Promise<unknown>) => {
		liveCalls.push(id)
		return fn(`sid-${id}`)
	},
}))

const transfer = await import(
	'../../src/lib/features/workspace/transfer/transfer.svelte'
)
const { transferStore } = transfer

const table = {
	id: 't1',
	name: 'users',
	schema: 'public',
	database: '',
	kind: 'table',
	rowCount: 0,
	columns: [],
	rows: [],
} as never

beforeEach(() => {
	liveCalls.length = 0
	requests.length = 0
	session.activeConnectionId = 'a'
	transfer.closeImport()
	transfer.closeExport()
})

test('import runs on the connection the dialog was opened on', async () => {
	transfer.beginImport(table)
	expect(transferStore.connectionId).toBe('a')
	session.activeConnectionId = 'b'
	await transfer.confirmImportFile({ path: '/tmp/u.csv', format: 'csv' } as never)
	await transfer.confirmImportRun({
		encoding: 'utf8',
		mapping: [],
		onDuplicate: 'error',
	} as never)
	expect(liveCalls).toEqual(['a', 'a'])
	expect(requests).toEqual([
		{ database: 'db-of-a', kind: 'preview' },
		{ database: 'db-of-a', kind: 'import' },
	])
	expect(transferStore.importTable).toBeNull()
	expect(transferStore.connectionId).toBeNull()
})

test('export runs on the connection the dialog was opened on', async () => {
	transfer.beginExport(table)
	session.activeConnectionId = 'b'
	await transfer.confirmExport({
		folder: '/tmp',
		fileName: 'u.csv',
		format: 'csv',
	} as never)
	expect(liveCalls).toEqual(['a'])
	expect(requests).toEqual([{ database: 'db-of-a', kind: 'export', tabId: null }])
	expect(transferStore.exportTable).toBeNull()
	expect(transfer.transferQueue.tasks.at(-1)?.status).toBe('done')
	expect(transfer.transferQueue.tasks.at(-1)?.rows).toBe(3)
})

test('an export can read through the open transaction of a query tab on its connection', async () => {
	session.openTabs = [
		{ id: 'q-other', kind: 'query', connectionId: 'b', title: 'Other' },
		{ id: 'q-1', kind: 'query', connectionId: 'a', title: 'Query 1' },
	]
	session.tabTxn = {
		'q-other': { pinned: true, inTransaction: true },
		'q-1': { pinned: true, inTransaction: true },
	}
	transfer.beginExport(table)
	expect(transferStore.exportTxnTab).toEqual({ id: 'q-1', title: 'Query 1' })
	await transfer.confirmExport({
		folder: '/tmp',
		fileName: 'u.csv',
		format: 'csv',
		throughTransaction: true,
	} as never)
	transfer.beginExport(table)
	await transfer.confirmExport({
		folder: '/tmp',
		fileName: 'u.csv',
		format: 'csv',
		throughTransaction: false,
	} as never)
	expect(requests.map((r) => r.tabId)).toEqual(['q-1', null])

	// No open transaction on this connection: nothing to offer.
	session.tabTxn = { 'q-1': { pinned: true, inTransaction: false } }
	transfer.beginExport(table)
	expect(transferStore.exportTxnTab).toBeNull()
	session.openTabs = []
	session.tabTxn = {}
})
