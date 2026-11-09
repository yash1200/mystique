/** Open / reload a table preview. Owned by the grid, not the workspace shell. */
import type {
	ColumnReference,
	TableMeta,
	TablePreviewFilter,
	TablePreviewCount,
	TablePreviewSort,
} from '#core/contracts'
import { cancelTabRun, errorMessage, finishTabRun, startTabRun } from '#core/ipc'
import {
	applyPreviewStats,
	catalogStore,
	currentDatabaseName,
	loadTablePreview,
	loadTablePreviewStats,
	selectDatabase,
} from '#features/workspace/catalog'
import {
	catalogsFor,
	engineBindsSessionToDatabase,
	findCatalogTable,
	patchCatalogTable,
	reopenTableAction,
	requireActiveConnection,
	resolveTablePreview,
	selectedDatabases,
	tableTabId,
	withLiveSession,
	workspaceSession,
	workspaceStudio,
} from '#features/workspace/session'
import { COUNT_BUDGET_MS, countState, type TableCountState } from './count'
import { tableSync } from './sync.svelte'

class TablePreview {
	hydratedTabs = $state<Record<string, boolean>>({})
	epochByTab = $state<Record<string, number>>({})
	/** Tab whose preview is in flight. Per-tab so a stale finish cannot hide another open. */
	loadingTabId = $state<string | null>(null)
	/** Background total count per tab: counting, exact, or estimated. */
	countByTab = $state<Record<string, TableCountState>>({})

	#setCount(tabId: string, state: TableCountState | null) {
		if (state) {
			this.countByTab = { ...this.countByTab, [tabId]: state }
		} else {
			const { [tabId]: _gone, ...rest } = this.countByTab
			this.countByTab = rest
		}
	}

	/**
	 * Start the tab's count. A newer count on the tab cancels this one on the
	 * server (one `count` run slot per tab). `budgetMs` null counts every row.
	 */
	#startCount(
		tabId: string,
		sessionId: string,
		table: TableMeta,
		filters: TablePreviewFilter[],
		codeFilter: string | null,
		budgetMs: number | null,
	): { run: string; result: Promise<TablePreviewCount> } {
		const run = startTabRun(tabId, 'count')
		this.#setCount(tabId, { status: 'counting', cancelling: false, exact: budgetMs == null })
		const result = loadTablePreviewStats(sessionId, table, filters, codeFilter, {
			executionId: run,
			budgetMs,
		})
		void result.then(
			() => finishTabRun(tabId, run),
			() => finishTabRun(tabId, run),
		)
		return { run, result }
	}

	/** Land a count if no newer count or open replaced it. */
	#landCount(
		tabId: string,
		connectionId: string,
		tableId: string,
		run: string,
		stats: TablePreviewCount,
	) {
		if (this.#countRun.get(tabId) !== run) return
		this.#countRun.delete(tabId)
		this.#setCount(tabId, countState(stats))
		const current = findCatalogTable(catalogsFor(catalogStore.catalogs, connectionId), tableId)
		if (!current) return
		catalogStore.catalogs = patchCatalogTable(
			catalogStore.catalogs,
			connectionId,
			tableId,
			applyPreviewStats(current, stats),
		)
	}

	/** The count run each tab is waiting on. */
	#countRun = new Map<string, string>()

	/** Stop the tab's running count. It lands as cancelled, with the estimate. */
	cancelCount(tabId: string) {
		const state = this.countByTab[tabId]
		if (state?.status !== 'counting') return
		this.#setCount(tabId, { ...state, cancelling: true })
		void cancelTabRun(tabId, 'count')
	}

	/** Count every row of the active table tab, with its current filters. */
	async countExactly(tabId: string) {
		const tab = workspaceSession.openTabs.find((t) => t.id === tabId)
		if (!tab || tab.kind !== 'table' || !tab.tableId) return
		const active = requireActiveConnection()
		if (!active || active.id !== tab.connectionId) return
		const table = findCatalogTable(catalogsFor(catalogStore.catalogs, active.id), tab.tableId)
		if (!table) return
		const preview = tab.preview
		const tableId = tab.tableId
		try {
			await withLiveSession(active.id, async (sid) => {
				const { run, result } = this.#startCount(
					tabId,
					sid,
					table,
					preview?.filters ?? [],
					preview?.codeFilter ?? null,
					null,
				)
				this.#countRun.set(tabId, run)
				const stats = await result
				this.#landCount(tabId, active.id, tableId, run, stats)
			})
		} catch {
			if (this.countByTab[tabId]?.status === 'counting') {
				this.#setCount(tabId, { status: 'estimated', estimate: null, reason: 'failed' })
			}
		}
	}

	async open(
		table: TableMeta,
		offset = 0,
		filters: TablePreviewFilter[] = [],
		sort: TablePreviewSort | null = null,
		codeFilter: string | null = null,
		replacePreview = false,
		force = false,
	) {
		const active = requireActiveConnection()
		if (!active) return
		const { id, session } = active
		workspaceStudio.sideMode = 'explorer'
		const tabId = tableTabId(id, table.id)
		if (!force && reopenTableAction(tabId) === 'focus') {
			workspaceSession.openTable(id, table.id, table.name)
			return
		}
		const existing = workspaceSession.openTabs.find((t) => t.id === tabId)
		const preview = resolveTablePreview(existing?.preview, {
			offset,
			filters,
			sort,
			codeFilter,
			replace: replacePreview,
		})
		workspaceSession.openTable(id, table.id, table.name)
		workspaceSession.setTablePreview(tabId, preview)

		const epoch = (this.epochByTab[tabId] ?? 0) + 1
		this.epochByTab = { ...this.epochByTab, [tabId]: epoch }
		const stillThis = () => this.epochByTab[tabId] === epoch

		this.loadingTabId = tabId
		catalogStore.setError(id, null)
		const executionId = startTabRun(tabId, 'preview')
		const count: { stats: Promise<TablePreviewCount> | null; run: string | null } = {
			stats: null,
			run: null,
		}
		try {
			const engine = String(session.profile.engine)
			// The total count starts with the page, not after it. It is applied
			// once the page has landed, so the page cannot overwrite it. A retry
			// on a reopened session starts a new count.
			const loaded = await withLiveSession(id, (sid) => {
				const started = this.#startCount(
					tabId,
					sid,
					table,
					preview.filters,
					preview.codeFilter,
					COUNT_BUDGET_MS,
				)
				// Read below; a page that fails leaves it unread.
				started.result.catch(() => {})
				count.stats = started.result
				count.run = started.run
				this.#countRun.set(tabId, started.run)
				return loadTablePreview(
					sid,
					engine,
					table,
					100,
					preview.offset,
					preview.filters,
					preview.sort,
					preview.codeFilter,
					executionId,
				)
			})
			if (!stillThis()) return
			catalogStore.catalogs = patchCatalogTable(
				catalogStore.catalogs,
				id,
				table.id,
				loaded,
			)
			if (workspaceSession.activeTabId === tabId) {
				tableSync.previewLoaded(tabId)
			}

			const run = count.run
			void count.stats
				?.then((stats) => {
					if (!stillThis() || !run) return
					this.#landCount(tabId, id, table.id, run, stats)
				})
				.catch(() => {
					/* pager stays approximate until the next refresh */
					if (run && this.#countRun.get(tabId) === run) {
						this.#countRun.delete(tabId)
						this.#setCount(tabId, { status: 'estimated', estimate: null, reason: 'failed' })
					}
				})
		} catch (e) {
			if (stillThis()) {
				// No page, no count: stop it and drop its label.
				if (count.run && this.#countRun.get(tabId) === count.run) {
					this.#countRun.delete(tabId)
					this.#setCount(tabId, null)
					void cancelTabRun(tabId, 'count')
				}
			}
			if (stillThis() && workspaceSession.activeTabId === tabId) {
				catalogStore.setError(id, errorMessage(e))
			}
		} finally {
			finishTabRun(tabId, executionId)
			if (stillThis() && this.loadingTabId === tabId) {
				this.loadingTabId = null
			}
		}
	}

	async reload(opts?: {
		offset?: number
		filters?: TablePreviewFilter[]
		codeFilter?: string
		sortColumn?: string
		sortDir?: TablePreviewSort['dir']
	}) {
		const tab = workspaceSession.activeTab
		if (!tab || tab.kind !== 'table' || !tab.tableId) return
		const table = findCatalogTable(
			catalogsFor(catalogStore.catalogs, tab.connectionId),
			tab.tableId,
		)
		if (!table) return
		if (tab.id) {
			const { [tab.id]: _done, ...rest } = this.hydratedTabs
			this.hydratedTabs = rest
		}
		const sort =
			opts?.sortColumn && opts?.sortDir
				? { column: opts.sortColumn, dir: opts.sortDir }
				: null
		await this.open(
			table,
			opts?.offset ?? 0,
			opts?.filters ?? [],
			sort,
			opts?.codeFilter ?? null,
			true,
			true,
		)
	}

	hydrateActive() {
		const tab = workspaceSession.activeTab
		const session = workspaceSession.activeSession
		if (!tab || tab.kind !== 'table' || !tab.tableId || !session) return
		const table = findCatalogTable(
			catalogsFor(catalogStore.catalogs, tab.connectionId),
			tab.tableId,
		)
		if (!table) return
		if (this.hydratedTabs[tab.id]) return
		if (table.columns.length > 0) {
			this.hydratedTabs = { ...this.hydratedTabs, [tab.id]: true }
			return
		}
		if (this.loadingTabId === tab.id) return
		this.hydratedTabs = { ...this.hydratedTabs, [tab.id]: true }
		void this.open(table)
	}

	async openReference(ref: ColumnReference, value: unknown) {
		if (value === null || value === undefined) return
		const active = requireActiveConnection()
		if (!active) return
		const target = findReferencedTable(ref)
		if (!target) {
			catalogStore.setError(
				active.id,
				`Referenced table “${ref.schema ? `${ref.schema}.` : ''}${ref.table}” is not in the explorer catalog.`,
			)
			return
		}
		const targetDb = (target.database || ref.database || '').trim()
		const curDb = currentDatabaseName()
		if (targetDb && curDb && targetDb !== curDb) {
			const engine = String(active.session.profile.engine)
			if (engineBindsSessionToDatabase(engine, active.session.capabilities)) {
				try {
					await selectDatabase(targetDb)
				} catch {
					/* selectDatabase reports the catalog error */
				}
			} else {
				selectedDatabases.set(active.id, targetDb)
			}
		}
		const filterValue =
			typeof value === 'string' || typeof value === 'number'
				? String(value)
				: JSON.stringify(value)
		await this.open(target, 0, [
			{ column: ref.column, op: 'eq', value: filterValue },
		])
	}
}

function findReferencedTable(ref: ColumnReference): TableMeta | null {
	const id = workspaceSession.activeConnectionId
	if (!id) return null
	const list = catalogsFor(catalogStore.catalogs, id)
	const refDb = (ref.database ?? '').trim()
	const refSchema = (ref.schema ?? '').trim()
	const refTable = ref.table.trim()
	const matches: TableMeta[] = []
	for (const db of list) {
		for (const t of db.tables) {
			if (t.name !== refTable) continue
			if (refSchema && t.schema && t.schema !== refSchema) continue
			if (refDb && t.database && t.database !== refDb) continue
			matches.push(t)
		}
	}
	if (matches.length === 0) return null
	const curDb = currentDatabaseName()
	return (
		matches.find((t) => refSchema && t.schema === refSchema) ??
		matches.find((t) => t.database === curDb) ??
		matches[0] ??
		null
	)
}

export const tablePreview = new TablePreview()
