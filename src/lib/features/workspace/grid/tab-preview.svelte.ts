/** Open / reload a table preview. Owned by the grid, not the workspace shell. */
import type {
	ColumnReference,
	TableMeta,
	TablePreviewFilter,
	TablePreviewSort,
} from '#core/contracts'
import { finishTabRun, isTauri, startTabRun } from '#core/ipc'
import {
	applyPreviewStats,
	loadTablePreview,
	loadTablePreviewStats,
	selectDatabase,
} from '#features/workspace/catalog'
import {
	catalogsFor,
	currentDatabaseName,
	engineBindsSessionToDatabase,
	findCatalogTable,
	patchCatalogTable,
	reopenTableAction,
	requireActiveConnection,
	resolveTablePreview,
	tableTabId,
	withLiveSession,
	workspaceSession,
	workspaceStudio,
} from '#features/workspace/shell'

class TablePreview {
	hydratedTabs = $state<Record<string, boolean>>({})
	epochByTab = $state<Record<string, number>>({})

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

		if (!isTauri()) return

		const epoch = (this.epochByTab[tabId] ?? 0) + 1
		this.epochByTab = { ...this.epochByTab, [tabId]: epoch }
		const stillThis = () => this.epochByTab[tabId] === epoch

		workspaceStudio.loadingTabId = tabId
		workspaceStudio.setCatalogError(null)
		const executionId = startTabRun(tabId)
		try {
			const engine = String(session.profile.engine)
			const loaded = await withLiveSession(id, (sid) =>
				loadTablePreview(
					sid,
					engine,
					table,
					100,
					preview.offset,
					preview.filters,
					preview.sort,
					preview.codeFilter,
					executionId,
				),
			)
			if (!stillThis()) return
			workspaceStudio.catalogs = patchCatalogTable(
				workspaceStudio.catalogs,
				id,
				table.id,
				loaded,
			)
			if (workspaceSession.activeTabId === tabId) {
				workspaceStudio.tablePreviewTick += 1
			}

			void withLiveSession(id, (sid) =>
				loadTablePreviewStats(sid, table, preview.filters, preview.codeFilter),
			)
				.then((stats) => {
					if (!stillThis()) return
					const current = findCatalogTable(
						catalogsFor(workspaceStudio.catalogs, id),
						table.id,
					)
					if (!current) return
					workspaceStudio.catalogs = patchCatalogTable(
						workspaceStudio.catalogs,
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
				workspaceStudio.setCatalogError(
					e instanceof Error ? e.message : String(e),
				)
			}
		} finally {
			finishTabRun(tabId, executionId)
			if (stillThis() && workspaceStudio.loadingTabId === tabId) {
				workspaceStudio.loadingTabId = null
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
			catalogsFor(workspaceStudio.catalogs, tab.connectionId),
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
		if (!isTauri()) {
			// No engine fetch. Mark settled so the pane does not spin on the leaf.
			if (!this.hydratedTabs[tab.id]) {
				this.hydratedTabs = { ...this.hydratedTabs, [tab.id]: true }
			}
			return
		}
		const table = findCatalogTable(
			catalogsFor(workspaceStudio.catalogs, tab.connectionId),
			tab.tableId,
		)
		if (!table) return
		if (this.hydratedTabs[tab.id]) return
		if (table.columns.length > 0) {
			this.hydratedTabs = { ...this.hydratedTabs, [tab.id]: true }
			return
		}
		if (workspaceStudio.loadingTabId === tab.id) return
		this.hydratedTabs = { ...this.hydratedTabs, [tab.id]: true }
		void this.open(table)
	}

	async openReference(ref: ColumnReference, value: unknown) {
		if (value === null || value === undefined) return
		const active = requireActiveConnection()
		if (!active) return
		const target = findReferencedTable(ref)
		if (!target) {
			workspaceStudio.setCatalogError(
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
				workspaceStudio.selectedDb = {
					...workspaceStudio.selectedDb,
					[active.id]: targetDb,
				}
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
	const list = catalogsFor(workspaceStudio.catalogs, id)
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
