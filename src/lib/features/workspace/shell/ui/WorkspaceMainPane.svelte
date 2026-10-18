<script lang="ts">
	import { EmptyState } from '#core/ui'
	import { randomLoadingPun } from '#core/util'
	import { familyFromCaps, resolveCapabilities } from '#engines'
	import { colorHex } from '#features/connections'
	import { AgentChatPane } from '#features/workspace/agent'
	import {
		ErWorkspace,
		hydrateActiveModel,
		openModelDiagram,
	} from '#features/workspace/er'
	import {
		hydrateActiveStructure,
		openObjectStructure,
		StructurePanel,
	} from '#features/workspace/explorer'
	import {
		DataGrid,
		RowInspector,
		tablePreview,
	} from '#features/workspace/grid'
	import { ServerMetricsView } from '#features/workspace/metrics'
	import {
		prependHistoryItem,
		QueryCanvas,
		querySave,
	} from '#features/workspace/query'
	import { cancelTabRun } from '#core/ipc'
	import { appPrefs } from '#lib/app'
	import { onMount } from 'svelte'
	import { reconnectActiveSession } from '../live-session'
	import {
		catalogsFor,
		findCatalogTable,
		objectKindById as kindsFromCatalog,
		selectedDatabaseName,
		structureCacheKey,
	} from '../lookup'
	import { modelCacheKey } from '../session-utils'
	import { workspaceSession } from '../session.svelte'
	import { workspaceStudio } from '../studio.svelte'
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

	const databases = $derived(catalogsFor(workspaceStudio.catalogs, activeId))
	const currentDatabaseName = $derived(
		selectedDatabaseName(workspaceStudio.selectedDb, activeId, databases),
	)
	const connectionTabs = $derived(
		activeId ? openTabs.filter((t) => t.connectionId === activeId) : [],
	)
	const objectKindById = $derived(kindsFromCatalog(databases))

	const gridTable = $derived(
		activeTab?.kind === 'table'
			? findCatalogTable(
					catalogsFor(workspaceStudio.catalogs, activeTab.connectionId),
					activeTab.tableId,
				)
			: null,
	)
	const tableLoading = $derived(workspaceStudio.loadingTabId === activeTabId)
	let tablePun = $state(randomLoadingPun())
	let diagramPun = $state(randomLoadingPun())
	let tablePunKey = ''
	let diagramPunKey = ''
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
		return (
			workspaceStudio.liveModels[
				modelCacheKey(activeId, db, activeTab.tableId)
			] ?? null
		)
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
		return (
			workspaceStudio.liveStructures[
				structureCacheKey(activeId, activeTab.tableId)
			] ?? null
		)
	})
	const structure = $derived(activeStructure?.data ?? null)
	const structureFetchedAt = $derived(activeStructure?.rev ?? null)
	const structureTable = $derived(
		activeTab?.kind === 'structure'
			? findCatalogTable(databases, activeTab.tableId)
			: null,
	)

	const catalogError = $derived(workspaceStudio.catalogError)
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
		const key = showTableLoading ? String(activeTabId ?? '') : ''
		if (key && key !== tablePunKey) {
			tablePun = randomLoadingPun()
		}
		tablePunKey = key
	})
	const modelError = $derived(workspaceStudio.modelError)
	const modelLoading = $derived(workspaceStudio.modelLoading)
	const structureLoading = $derived(workspaceStudio.structureLoading)

	$effect(() => {
		const show = modelLoading && !activeModel
		const key = show ? String(activeTabId ?? '') : ''
		if (key && key !== diagramPunKey) {
			diagramPun = randomLoadingPun()
		}
		diagramPunKey = key
	})
	const structureError = $derived(workspaceStudio.structureError)
	const structureCaps = $derived(
		activeSession
			? resolveCapabilities(
					String(activeSession.profile.engine),
					activeSession.capabilities,
				)
			: null,
	)
	const rowSyncTick = $derived(workspaceStudio.rowSyncTick)
	const tablePreviewTick = $derived(workspaceStudio.tablePreviewTick)
	const baselineEpoch = $derived(workspaceStudio.baselineEpoch)
	const querysaving = $derived(workspaceStudio.querySaving)

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
		if (activeSession?.profile.readOnly) return
		if (rowSelectionIndex === null || !gridTable) return
		const i = rowSelectionIndex
		const current = gridTable.rows[i] ?? {}
		gridTable.rows[i] = { ...current, [column]: value }
		workspaceStudio.rowSyncTick += 1
	}

	$effect(() => {
		void activeTabId
		void gridTable?.id
		tablePreview.hydrateActive()
	})

	onMount(() => {
		const onKey = (e: KeyboardEvent) => {
			if (e.key !== 'Escape' || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey)
				return
			const target = e.target
			if (target instanceof HTMLElement) {
				if (
					target.closest(
						'[role="dialog"], input, textarea, [contenteditable="true"]',
					)
				) {
					return
				}
			}
			const loading = workspaceStudio.loadingTabId
			if (!loading) return
			e.preventDefault()
			void cancelTabRun(loading)
		}
		window.addEventListener('keydown', onKey, true)
		return () => window.removeEventListener('keydown', onKey, true)
	})

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
		<TableTabs
			tabs={connectionTabs}
			{activeTabId}
			connectionColor={activeSession
				? colorHex(activeSession.profile.colorId)
				: null}
			production={activeSession?.profile.environment === 'production'}
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
	{/if}

	<div class="flex min-h-0 flex-1">
		{#if activeSession && activeTab?.kind === 'query'}
			<div class="flex min-h-0 min-w-0 flex-1">
				<div class="flex min-h-0 min-w-0 flex-1 flex-col">
					{#key activeTab.id}
						<QueryCanvas
							tabId={activeTab.id}
							connectionId={activeSession.profile.id}
							connectionName={activeSession.profile.name}
							engine={String(activeSession.profile.engine)}
							capabilities={activeSession.capabilities}
							profileReadOnly={!!activeSession.profile.readOnly}
							engineSessionId={activeSession.engineSessionId}
							database={currentDatabaseName}
							text={workspaceSession.queryDrafts[activeTab.id] ?? ''}
							dirty={!!activeTab.dirty}
							saving={querysaving}
							ontextchange={(v) =>
								workspaceSession.setQueryDraft(activeTab.id, v)}
							onhistory={prependHistoryItem}
							onsave={() => void querySave.saveActive()}
							onreconnect={reconnectActiveSession}
						/>
					{/key}
				</div>
				{#if appPrefs.agentChatOpen}
					<div
						class="flex h-full shrink-0"
						style="width: {appPrefs.agentChatPanelWidth}px"
					>
						<AgentChatPane
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
						<EmptyState title="Building diagram…" description={diagramPun} />
					</div>
				{:else if activeModel}
					{#key `${activeTab.id}:${activeModel.rev}`}
						<ErWorkspace
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
				<ServerMetricsView
					engine={String(activeSession.profile.engine)}
					engineSessionId={activeSession.engineSessionId ?? null}
					profileReadOnly={!!activeSession.profile.readOnly}
					capabilities={activeSession.capabilities}
					production={activeSession.profile.environment === 'production'}
				/>
			{/key}
		{:else if activeSession && activeTab?.kind === 'structure'}
			{#key activeTab.id}
				<StructurePanel
					{structure}
					loading={structureLoading}
					error={structureError}
					lastFetchedAt={structureFetchedAt}
					canEdit={structureCaps?.supportsStructureEdit === true}
					readOnly={!!activeSession.profile.readOnly}
					showForeignKeys={structureCaps?.supportsForeignKeys === true}
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
						<EmptyState title="Loading table…" description={tablePun} />
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
							{rowSyncTick}
							{tablePreviewTick}
							onbaselinechange={() => {
								workspaceStudio.baselineEpoch += 1
							}}
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
							profileReadOnly={!!activeSession.profile.readOnly}
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
						{baselineEpoch}
						readOnly={!!activeSession.profile.readOnly}
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
