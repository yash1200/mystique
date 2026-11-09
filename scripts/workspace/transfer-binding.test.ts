import { beforeEach, expect, test, vi } from 'vitest'

// D-06: an import or export dialog is bound to the connection it was opened
// on. Switching the active connection while it is open must not send the run
// to the other connection.
const session = {
	activeConnectionId: 'a' as string | null,
	openTabs: [] as { id: string; dirty?: boolean }[],
}
const liveCalls: string[] = []
const requests: { database: string | null; kind: string }[] = []

vi.mock('#core/ipc', () => ({
	errorMessage: (e: unknown) => String(e),
	startTabRun: () => 'exec-1',
	finishTabRun: () => {},
	cancelTabRun: async () => {},
	previewEngineImport: async (req: { database: string | null }) => {
		requests.push({ database: req.database, kind: 'preview' })
		return { columns: [], rows: [] }
	},
	importEngineObject: async (req: { database: string | null }) => {
		requests.push({ database: req.database, kind: 'import' })
	},
	exportEngineObject: async (req: { database: string | null }) => {
		requests.push({ database: req.database, kind: 'export' })
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
	expect(requests).toEqual([{ database: 'db-of-a', kind: 'export' }])
	expect(transferStore.exportTable).toBeNull()
})
