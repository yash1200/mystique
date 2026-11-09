/** Open / reload a table preview. Owned by the grid, not the workspace shell. */
import type {
	ColumnReference,
	TableMeta,
	TablePreviewFilter,
	TablePreviewSort,
	TablePreviewStats,
} from '#core/contracts'
import { errorMessage, finishTabRun, startTabRun } from '#core/ipc'
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
import { tableSync } from './sync.svelte'

class TablePreview {
	hydratedTabs = $state<Record<string, boolean>>({})
	epochByTab = $state<Record<string, number>>({})
	/** Tab whose preview is in flight. Per-tab so a stale finish cannot hide another open. */
	loadingTabId = $state<string | null>(null)

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
		try {
			const engine = String(session.profile.engine)
			// The total count starts with the page, not after it. It is applied
			// once the page has landed, so the page cannot overwrite it. A retry
			// on a reopened session starts a new count.
			const count: { stats: Promise<TablePreviewStats> | null } = {
				stats: null,
			}
			const loaded = await withLiveSession(id, (sid) => {
				const stats = loadTablePreviewStats(
					sid,
					table,
					preview.filters,
					preview.codeFilter,
				)
				// Read below; a page that fails leaves it unread.
				stats.catch(() => {})
				count.stats = stats
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

			void count.stats
				?.then((stats) => {
					if (!stillThis()) return
					const current = findCatalogTable(
						catalogsFor(catalogStore.catalogs, id),
						table.id,
					)
					if (!current) return
					catalogStore.catalogs = patchCatalogTable(
						catalogStore.catalogs,
						id,
						table.id,
						applyPreviewStats(current, stats),
					)
				})
				.catch(() => {
					/* pager stays approximate until the next refresh */
				})
		} catch (e) {
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
