<script lang="ts" module>
	import { lazy } from '#core/ui'

	// Panes most sessions never open load with their first use.
	const AgentChatPane = lazy(() =>
		import('#features/workspace/agent').then((m) => ({
			default: m.AgentChatPane,
		})),
	)
	const ServerMetricsView = lazy(() =>
		import('#features/workspace/metrics').then((m) => ({
			default: m.ServerMetricsView,
		})),
	)
</script>

<script lang="ts">
	import { commands } from '#core/commands'
	import { EmptyState, Icon, Lazy } from '#core/ui'
	import { familyFromCaps, resolveCapabilities } from '#engines'
	import { colorHex, isProductionDatabase } from '#features/connections'
	import { catalogStore, databaseNameFor } from '#features/workspace/catalog'
	import {
		ErWorkspace,
		hydrateActiveModel,
		modelKey,
		modelStore,
		openModelDiagram,
	} from '#features/workspace/er'
	import { StructurePanel } from '#features/workspace/explorer'
	import {
		DataGrid,
		RowInspector,
		tablePreview,
		tableSync,
	} from '#features/workspace/table'
	import {
		hydrateActiveStructure,
		openObjectStructure,
		structureStore,
	} from '#features/workspace/structure'
	import {
		prefetchQueryEditor,
		prependHistoryItem,
		pruneEditorStates,
		QueryCanvas,
		querySave,
	} from '#features/workspace/query'
	import { cancelTabRun } from '#core/ipc'
	import { appPrefs, cmd } from '#lib/app'
	import { onMount } from 'svelte'
	import {
		reconnectActiveSession,
		reconnectSession,
		findCatalogTable,
		objectKindById as kindsFromCatalog,
		timedUnlocks,
		workspaceSession,
		workspaceStudio,
	} from '#features/workspace/session'
	import SessionMarks from './SessionMarks.svelte'
	import TableTabs from './TableTabs.svelte'
	import WorkspaceEmpty from './WorkspaceEmpty.svelte'

	interface Props {
		/** Close request (may prompt if dirty). */
		onclosetab?: (id: string) => void
		/** Close several tabs in order (dirty prompts stay sequential). */
		onclosemany?: (ids: string[]) => void
	}

	let { onclosetab, onclosemany }: Props = $props()

	const sideMode = $derived(workspaceStudio.sideMode)
	const activeId = $derived(workspaceSession.activeConnectionId)
	const activeSession = $derived(workspaceSession.activeSession)
	const activeTabId = $derived(workspaceSession.activeTabId)
	const activeTab = $derived(workspaceSession.activeTab)
	const openTabs = $derived(workspaceSession.openTabs)
	const selectedRow = $derived(workspaceSession.selectedRow)

	const databases = $derived(catalogStore.databases(activeId))
	const currentDatabaseName = $derived(databaseNameFor(activeId))
	/** Read-only unless a timed unlock runs on this session. */
	const readOnlyNow = $derived(
		timedUnlocks.readOnly(
			activeSession?.profile.readOnly,
			activeSession?.engineSessionId,
		),
	)
	/** The current database is production (connection or per-database mark). */
	const productionNow = $derived(
		!!activeSession &&
			isProductionDatabase(activeSession.profile, currentDatabaseName),
	)
	const connectionTabs = $derived(
		activeId ? openTabs.filter((t) => t.connectionId === activeId) : [],
	)
	const objectKindById = $derived(kindsFromCatalog(databases))

	const gridTable = $derived(
		activeTab?.kind === 'table'
			? findCatalogTable(
					catalogStore.databases(activeTab.connectionId),
					activeTab.tableId,
				)
			: null,
	)
	const tableLoading = $derived(tablePreview.loadingTabId === activeTabId)
	const rowSelectionIndex = $derived.by(() => {
		if (!selectedRow || !activeTab || activeTab.kind !== 'table') return null
		if (
			selectedRow.connectionId === activeTab.connectionId &&
			selectedRow.tableId === activeTab.tableId
		) {
			return selectedRow.rowIndex
		}
		return null
	})
	const inspectorData = $derived.by(() => {
		if (sideMode !== 'explorer') return null
		if (rowSelectionIndex === null || !gridTable) return null
		const row = gridTable.rows[rowSelectionIndex]
		if (!row) return null
		const title = gridTable.schema
			? `${gridTable.database}.${gridTable.schema}.${gridTable.name}`
			: `${gridTable.database}.${gridTable.name}`
		return {
			tableTitle: title,
			columns: gridTable.columns,
			row,
			rowIndex: rowSelectionIndex,
		}
	})

	const activeModel = $derived.by(() => {
		if (!activeTab || activeTab.kind !== 'model' || !activeId) return null
		const db = activeTab.modelDatabase ?? currentDatabaseName ?? ''
		return modelStore.get(activeId, modelKey(db, activeTab.tableId)) ?? null
	})
	const activeStructure = $derived.by(() => {
		if (
			!activeId ||
			!activeTab ||
			activeTab.kind !== 'structure' ||
			!activeTab.tableId
		) {
			return null
		}
		return structureStore.get(activeId, activeTab.tableId) ?? null
	})
	const structure = $derived(activeStructure?.data ?? null)
	const structureFetchedAt = $derived(activeStructure?.rev ?? null)
	const structureTable = $derived(
		activeTab?.kind === 'structure'
			? findCatalogTable(databases, activeTab.tableId)
			: null,
	)

	const catalogError = $derived(catalogStore.statusOf(activeId).error)
	// A restored tab renders before hydrate starts. The catalog leaf has no
	// columns yet. Mounting the grid then stores an empty edit buffer and the
	// real preview is ignored on remount. Wait until the fetch settles.
	const tablePreviewSettled = $derived(
		!!activeTabId &&
			tablePreview.hydratedTabs[activeTabId] === true &&
			!tableLoading,
	)
	const showTableLoading = $derived(
		!catalogError &&
			((tableLoading && (!gridTable || gridTable.columns.length === 0)) ||
				(!!gridTable &&
					gridTable.columns.length === 0 &&
					!tablePreviewSettled)),
	)

	$effect(() => {
		pruneEditorStates(workspaceSession.openTabs.map((tab) => tab.id))
	})

	const modelError = $derived(modelStore.of(activeId).error)
	const modelLoading = $derived(modelStore.of(activeId).loading)
	const structureLoading = $derived(structureStore.of(activeId).loading)

	const structureError = $derived(structureStore.of(activeId).error)
	const structureCaps = $derived(
		activeSession
			? resolveCapabilities(
					String(activeSession.profile.engine),
					activeSession.capabilities,
				)
			: null,
	)
	const querysaving = $derived(
		!!activeTabId && querySave.savingTabId === activeTabId,
	)

	let gridOriginals = $state<Record<string, unknown>[]>([])

	const tabPreview = $derived(
		activeTab?.kind === 'table' ? (activeTab.preview ?? null) : null,
	)
	const seedFilters = $derived(
		tabPreview?.filters.length ? tabPreview.filters : null,
	)
	const seedCodeFilter = $derived(tabPreview?.codeFilter ?? null)
	const seedSort = $derived(tabPreview?.sort ?? null)

	function onSelectRow(index: number) {
		if (!activeTab || activeTab.kind !== 'table' || !activeTab.tableId) return
		workspaceSession.selectRow(activeTab.connectionId, activeTab.tableId, index)
	}

	function onInspectorChange(column: string, value: unknown) {
		if (readOnlyNow) return
		if (rowSelectionIndex === null || !gridTable) return
		const i = rowSelectionIndex
		const current = gridTable.rows[i] ?? {}
		gridTable.rows[i] = { ...current, [column]: value }
		if (activeTabId) tableSync.rowsEdited(activeTabId)
	}

	$effect(() => {
		void activeTabId
		void gridTable?.id
		tablePreview.hydrateActive()
	})

	// A query tab warms its catalog columns and language code while the
	// editor chunk is still loading, so the first completion is instant.
	let prefetchedQuery = ''
	$effect(() => {
		const session = activeSession
		const tab = activeTab
		if (!session || tab?.kind !== 'query') return
		const sessionId = session.engineSessionId
		const database = currentDatabaseName
		const key = `${tab.id}\0${sessionId ?? ''}\0${database}`
		if (key === prefetchedQuery) return
		prefetchedQuery = key
		prefetchQueryEditor({
			engine: String(session.profile.engine),
			capabilities: session.capabilities,
			sessionId,
			database,
		})
	})

	onMount(() =>
		commands.register({
			...cmd.cancelLoad,
			// An open context menu takes Esc first.
			when: () =>
				tablePreview.loadingTabId != null &&
				!document.querySelector('[role="menu"]'),
			run: () => {
				const loading = tablePreview.loadingTabId
				if (loading) void cancelTabRun(loading, 'preview')
			},
		}),
	)

	$effect(() => {
		void activeTabId
		hydrateActiveStructure()
	})

	$effect(() => {
		void activeTabId
		hydrateActiveModel()
	})
</script>

<main class="flex min-w-0 flex-1 flex-col">
	<!-- Tabs stay visible: explorer / saved / history all open content in the editor. -->
	{#if (sideMode === 'explorer' || sideMode === 'saved-queries' || sideMode === 'history') && activeId}
		<div
			class="flex min-w-0 shrink-0 [&>[role=tablist]]:min-w-0 [&>[role=tablist]]:flex-1"
		>
			<TableTabs
				tabs={connectionTabs}
				{activeTabId}
				connectionColor={activeSession
					? colorHex(activeSession.profile.colorId)
					: null}
				production={productionNow}
				onactivate={(id) => workspaceSession.setActiveTab(id)}
				onclose={(id) =>
					onclosetab ? onclosetab(id) : workspaceSession.closeTab(id)}
				onclosemany={(ids) =>
					onclosemany
						? onclosemany(ids)
						: ids.forEach((id) =>
								onclosetab ? onclosetab(id) : workspaceSession.closeTab(id),
							)}
				onopenquery={() => workspaceSession.openNewQuery(activeId)}
				onreorder={(movedId, beforeId) =>
					workspaceSession.reorderTabs(movedId, beforeId)}
				{objectKindById}
			/>
			{#if activeSession}
				<SessionMarks
					profile={activeSession.profile}
					engineSessionId={activeSession.engineSessionId ?? null}
					database={currentDatabaseName}
				/>
			{/if}
		</div>
	{/if}

	<div class="flex min-h-0 flex-1">
		{#if activeSession && activeTab?.kind === 'query'}
			<div class="flex min-h-0 min-w-0 flex-1">
				<div class="flex min-h-0 min-w-0 flex-1 flex-col">
					<Lazy
						component={QueryCanvas}
						tabId={activeTab.id}
						connectionId={activeSession.profile.id}
						connectionName={activeSession.profile.name}
						engine={String(activeSession.profile.engine)}
						capabilities={activeSession.capabilities}
						readOnly={readOnlyNow}
						engineSessionId={activeSession.engineSessionId}
						database={currentDatabaseName}
						text={workspaceSession.queryDrafts[activeTab.id] ?? ''}
						dirty={!!activeTab.dirty}
						saving={querysaving}
						ontextchange={(v) =>
							workspaceSession.setQueryDraft(activeTab.id, v)}
						onhistory={prependHistoryItem}
						onsave={() => void querySave.saveActive()}
						onreconnect={reconnectSession}
					/>
				</div>
				{#if appPrefs.agentChatOpen}
					<div
						class="flex h-full shrink-0"
						style="width: {appPrefs.agentChatPanelWidth}px"
					>
						<Lazy
							component={AgentChatPane}
							tabId={activeTab.id}
							connectionId={activeSession.profile.id}
							connectionName={activeSession.profile.name}
							engine={String(activeSession.profile.engine)}
							database={currentDatabaseName}
						/>
					</div>
				{/if}
			</div>
		{:else if activeSession && activeTab?.kind === 'model'}
			<div class="relative flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash">
				{#if modelError}
					<div
						class="shrink-0 border-b border-line bg-danger/10 px-3 py-2 text-caption text-danger"
					>
						{modelError}
						<button
							type="button"
							class="ml-2 border-none bg-transparent text-accent underline"
							onclick={() => void openModelDiagram(true)}
						>
							Retry
						</button>
					</div>
				{/if}
				{#if modelLoading && !activeModel}
					<div class="flex flex-1 items-center justify-center p-8">
						<div class="flex flex-col items-center gap-3" aria-busy="true">
							<Icon name="refresh" class="size-5 animate-spin text-fg-muted" />
							<EmptyState title="Building diagram…" />
						</div>
					</div>
				{:else if activeModel}
					{#key `${activeTab.id}:${activeModel.rev}`}
						<Lazy
							component={ErWorkspace}
							family={activeModel.diagram.family ||
								familyFromCaps(
									resolveCapabilities(
										String(activeSession.profile.engine),
										activeSession.capabilities,
									),
								)}
							initialDiagram={activeModel.diagram}
							initialName={activeModel.title}
							focusNodeId={activeModel.focusNodeId}
							onrefresh={() => void openModelDiagram(true)}
							refreshing={modelLoading}
						/>
					{/key}
				{:else}
					<div class="flex flex-1 items-center justify-center p-8">
						<EmptyState
							title={resolveCapabilities(
								String(activeSession.profile.engine),
								activeSession.capabilities,
							).supportsErDiagram
								? 'ER diagram'
								: 'Model'}
							description="Click the diagram icon on a table, or right-click and choose View ERD."
						/>
					</div>
				{/if}
			</div>
		{:else if activeSession && activeTab?.kind === 'metrics'}
			{#key activeTab.id}
				<Lazy
					component={ServerMetricsView}
					engine={String(activeSession.profile.engine)}
					engineSessionId={activeSession.engineSessionId ?? null}
					readOnly={readOnlyNow}
					capabilities={activeSession.capabilities}
					production={productionNow}
				/>
			{/key}
		{:else if activeSession && activeTab?.kind === 'structure'}
			{#key activeTab.id}
				<Lazy
					component={StructurePanel}
					{structure}
					loading={structureLoading}
					error={structureError}
					lastFetchedAt={structureFetchedAt}
					canEdit={structureCaps?.supportsStructureEdit === true}
					readOnly={readOnlyNow}
					showForeignKeys={structureCaps?.supportsForeignKeys === true}
					editLimited={structureCaps?.structureEditLimited === true}
					connectionId={activeSession.profile.id}
					engine={String(activeSession.profile.engine)}
					table={structureTable}
					onretry={() => {
						if (structureTable) void openObjectStructure(structureTable, true)
					}}
					onopentable={structureTable
						? () => void tablePreview.open(structureTable)
						: undefined}
				/>
			{/key}
		{:else if activeSession && activeTab?.kind === 'table'}
			<div class="relative flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash">
				{#if showTableLoading}
					<div class="flex flex-1 items-center justify-center p-8">
						<div class="flex flex-col items-center gap-3" aria-busy="true">
							<Icon name="refresh" class="size-5 animate-spin text-fg-muted" />
							<EmptyState title="Loading table…" />
						</div>
					</div>
				{:else if gridTable}
					<!-- Key on the tab, not the seed. Including seedFilters remounted
					     the grid on every filter apply and marked every cell unsaved.
					     Tab id keeps filter $state from leaking across tables. -->
					{#key activeTab.id}
						<DataGrid
							tabId={activeTab.id}
							table={gridTable}
							selectedRowIndex={rowSelectionIndex}
							onselectrow={onSelectRow}
							engine={String(activeSession.profile.engine)}
							connectionId={activeSession.profile.id}
							sqlDialect={resolveCapabilities(
								String(activeSession.profile.engine),
								activeSession.capabilities,
							).sqlDialect}
							supportsDistinct={resolveCapabilities(
								String(activeSession.profile.engine),
								activeSession.capabilities,
							).supportsDistinctValues !== false}
							sessionId={activeSession.engineSessionId}
							readOnly={readOnlyNow}
							production={productionNow}
							connectionName={activeSession.profile.name}
							onreload={(opts) => tablePreview.reload(opts)}
							onreconnect={reconnectActiveSession}
							{seedFilters}
							{seedCodeFilter}
							{seedSort}
							codeFilterPlaceholder={resolveCapabilities(
								String(activeSession.profile.engine),
								activeSession.capabilities,
							).codeFilterPlaceholder}
							onopenreference={(ref, value) =>
								void tablePreview.openReference(ref, value)}
							onopenstructure={() => void openObjectStructure(gridTable)}
							ondirtychange={(dirty) =>
								workspaceSession.setTabDirty(activeTab.id, dirty)}
							bind:originals={gridOriginals}
						/>
					{/key}
				{:else}
					<div class="flex flex-1 items-center justify-center p-8">
						<EmptyState
							title={catalogError ? 'Could not load table' : 'Open a table'}
							description={catalogError
								? `${catalogError} Pick another table from the explorer.`
								: 'Double-click a table in the explorer to load rows from the database.'}
						/>
					</div>
				{/if}
			</div>
			{#if inspectorData}
				{#key `${inspectorData.tableTitle}:${inspectorData.rowIndex}`}
					<RowInspector
						tableTitle={inspectorData.tableTitle}
						columns={inspectorData.columns}
						row={inspectorData.row}
						rowIndex={inspectorData.rowIndex}
						savedRow={gridOriginals[inspectorData.rowIndex] ?? null}
						tabId={activeTab.id}
						readOnly={readOnlyNow}
						onclose={() => workspaceSession.clearRowSelection()}
						onchange={onInspectorChange}
					/>
				{/key}
			{/if}
		{:else if activeSession}
			<WorkspaceEmpty
				profile={activeSession.profile}
				onnewquery={() => {
					if (activeId) workspaceSession.openNewQuery(activeId)
				}}
			/>
		{:else}
			<div class="flex flex-1 items-center justify-center p-8">
				<EmptyState
					title="No session selected"
					description="Pick a connection from the rail or return home to connect."
				/>
			</div>
		{/if}
	</div>
</main>
