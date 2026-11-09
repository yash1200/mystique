import { beforeEach, describe, expect, test, vi } from 'vitest'

import type { TableMeta } from '../../src/lib/core/contracts'

// E-09: the grid's total count starts with the page, not after it, and is
// applied after the page so the page cannot overwrite it.
const order: string[] = []
const patches: unknown[] = []
let releasePage: (page: unknown) => void = () => {}
let failPage: (error: Error) => void = () => {}

vi.mock('#core/ipc', () => ({
	errorMessage: (e: unknown) => String(e),
	startTabRun: () => 'run',
	finishTabRun: () => {},
}))
vi.mock('#features/workspace/catalog', () => ({
	applyPreviewStats: (table: object, stats: { totalRows: number }) => ({
		...table,
		rowCount: stats.totalRows,
	}),
	catalogStore: { catalogs: [], setError: () => {} },
	currentDatabaseName: () => '',
	loadTablePreview: () => {
		order.push('page')
		return new Promise((resolve, reject) => {
			releasePage = resolve
			failPage = reject
		})
	},
	loadTablePreviewStats: () => {
		order.push('count')
		return Promise.resolve({ totalRows: 42, totalKnown: true, references: [] })
	},
	selectDatabase: async () => {},
}))
vi.mock('#features/workspace/session', () => ({
	catalogsFor: () => [],
	engineBindsSessionToDatabase: () => false,
	findCatalogTable: () => ({ id: 't', name: 't' }),
	patchCatalogTable: (_c: unknown, _id: string, _t: string, patch: unknown) => {
		patches.push(patch)
		return []
	},
	reopenTableAction: () => 'load',
	requireActiveConnection: () => ({
		id: 'c',
		session: { profile: { engine: 'mongodb' } },
	}),
	resolveTablePreview: () => ({
		offset: 0,
		filters: [],
		sort: null,
		codeFilter: null,
	}),
	selectedDatabases: { set: () => {} },
	tableTabId: () => 'tab',
	withLiveSession: (_id: string, fn: (sid: string) => Promise<unknown>) =>
		fn('sid'),
	workspaceSession: {
		openTabs: [],
		openTable: () => {},
		setTablePreview: () => {},
		activeTabId: 'tab',
	},
	workspaceStudio: { sideMode: '' },
}))
vi.mock('../../src/lib/features/workspace/table/sync.svelte', () => ({
	tableSync: { previewLoaded: () => {} },
}))

const { tablePreview } = await import(
	'../../src/lib/features/workspace/table/tab-preview.svelte'
)

const table = { id: 't', name: 't' } as TableMeta
const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

beforeEach(() => {
	order.length = 0
	patches.length = 0
})

describe('table total count', () => {
	test('starts with the page and lands after it', async () => {
		const opening = tablePreview.open(table, 0, [], null, null, false, true)
		await settle()
		// Both requests are out while the page is still loading.
		expect(order).toEqual(['count', 'page'])
		expect(patches).toEqual([])
		releasePage({ id: 't', rows: 'page' })
		await opening
		await settle()
		expect(patches).toEqual([
			{ id: 't', rows: 'page' },
			{ id: 't', name: 't', rowCount: 42 },
		])
	})

	test('a failed page drops the count', async () => {
		const opening = tablePreview.open(table, 0, [], null, null, false, true)
		await settle()
		failPage(new Error('boom'))
		await opening
		await settle()
		expect(order).toEqual(['count', 'page'])
		expect(patches).toEqual([])
	})
})
