import { beforeEach, describe, expect, test, vi } from 'vitest'

import type { TableMeta } from '../../src/lib/core/contracts'

// E-09: the grid's total count starts with the page, not after it, and is
// applied after the page so the page cannot overwrite it.
const order: string[] = []
const patches: unknown[] = []
let releasePage: (page: unknown) => void = () => {}
let failPage: (error: Error) => void = () => {}
const cancelled: string[] = []
const countCalls: { budgetMs?: number | null; executionId?: string | null }[] = []
let countResult: unknown = { totalRows: 42, totalKnown: true, references: [], outcome: 'exact' }

vi.mock('#core/ipc', () => ({
	errorMessage: (e: unknown) => String(e),
	startTabRun: (_tab: string, kind: string) => `run-${kind}`,
	finishTabRun: () => {},
	cancelTabRun: async (_tab: string, kind: string) => {
		cancelled.push(kind)
	},
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
	loadTablePreviewStats: (
		_sid: string,
		_table: unknown,
		_filters: unknown,
		_code: unknown,
		opts: { budgetMs?: number | null; executionId?: string | null },
	) => {
		order.push('count')
		countCalls.push(opts)
		return Promise.resolve(countResult)
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
	cancelled.length = 0
	countCalls.length = 0
	countResult = { totalRows: 42, totalKnown: true, references: [], outcome: 'exact' }
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
		// Budgeted, cancellable by its own run id.
		expect(countCalls).toEqual([{ executionId: 'run-count', budgetMs: 3000 }])
		expect(tablePreview.countByTab.tab).toEqual({ status: 'exact', total: 42 })
	})

	test('a slow count lands as the estimate', async () => {
		countResult = {
			totalRows: 0,
			totalKnown: false,
			references: [],
			outcome: 'over_budget',
			estimatedRows: 1_200_000,
		}
		const opening = tablePreview.open(table, 0, [], null, null, false, true)
		await settle()
		expect(tablePreview.countByTab.tab).toMatchObject({ status: 'counting' })
		releasePage({ id: 't', rows: 'page' })
		await opening
		await settle()
		expect(tablePreview.countByTab.tab).toEqual({
			status: 'estimated',
			estimate: 1_200_000,
			reason: 'over_budget',
		})
	})

	test('a failed page drops the count', async () => {
		const opening = tablePreview.open(table, 0, [], null, null, false, true)
		await settle()
		failPage(new Error('boom'))
		await opening
		await settle()
		expect(order).toEqual(['count', 'page'])
		expect(patches).toEqual([])
		// The orphaned count is stopped on the server.
		expect(cancelled).toEqual(['count'])
		expect(tablePreview.countByTab.tab).toBeUndefined()
	})
})
