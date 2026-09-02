<script lang="ts">
	import type {
		CatalogObjectKind,
		ColumnReference,
		ConnectedSession,
		ObjectStructure,
		TableMeta,
		TablePreviewFilter,
	} from '$core/contracts'
	import { EmptyState } from '$core/ui'
	import { familyFromCaps, resolveCapabilities } from '$engines'
	import { colorHex } from '$features/connections'
	import { AgentChatPane } from '$features/workspace/agent'
	import type { ErDiagramState } from '$features/workspace/er'
	import { ErWorkspace } from '$features/workspace/er'
	import { StructurePanel } from '$features/workspace/explorer'
	import { DataGrid, RowInspector } from '$features/workspace/grid'
	import { ServerMetricsView } from '$features/workspace/metrics'
	import type { QueryHistoryItem } from '$features/workspace/query'
	import { QueryCanvas } from '$features/workspace/query'
	import { appPrefs } from '$lib/app'
	import { workspaceSession } from '../session.svelte'
	import type { WorkspaceTab } from '../types'
	import type { SideNavMode } from './SessionsRail.svelte'
	import TableTabs from './TableTabs.svelte'
	import WorkspaceEmpty from './WorkspaceEmpty.svelte'

	interface Props {
		sideMode: SideNavMode
		activeId: string | null
		activeSession: ConnectedSession | null
		connectionTabs: WorkspaceTab[]
		activeTabId: string | null
		activeTab: WorkspaceTab | null
		currentDatabaseName: string
		catalogError: string | null
		modelError: string | null
		modelLoading: boolean
		activeModel: {
			diagram: ErDiagramState
			title: string
			summary: string
			rev: number
			focusNodeId?: string
		} | null
		structure?: ObjectStructure | null
		structureLoading?: boolean
		structureError?: string | null
		/** Epoch ms of the last engine structure fetch. */
		structureFetchedAt?: number | null
		onopentable?: () => void
		gridTable: TableMeta | null
		tableLoading: boolean
		rowSelectionIndex: number | null
		rowSyncTick: number
		/** Server replaced the table preview (filter/sort/page/open). */
		tablePreviewTick?: number
		baselineEpoch: number
		inspectorData: {
			tableTitle: string
			columns: TableMeta['columns']
			row: Record<string, unknown>
			rowIndex: number
		} | null
		onhistory: (item?: QueryHistoryItem | null) => void
		onbaselinechange: () => void
		onselectrow: (index: number) => void
		onopenmodelretry: () => void
		onopenstructureretry?: () => void
		oninspectorchange: (column: string, value: unknown) => void
		onsavequery?: () => void
		querysaving?: boolean
		/** Close request (may prompt if dirty). */
		onclosetab?: (id: string) => void
		/** Close several tabs in order (dirty prompts stay sequential). */
		onclosemany?: (ids: string[]) => void
		onreloadtable?: (opts?: {
			offset?: number
			filters?: TablePreviewFilter[]
			sortColumn?: string
			sortDir?: 'asc' | 'desc'
		}) => Promise<void>
		onreconnect?: () => Promise<string | null>
		onopenreference?: (ref: ColumnReference, value: unknown) => void
		onopenstructure?: (table: TableMeta) => void
		objectKindById?: Record<string, CatalogObjectKind>
	}

	let {
		sideMode,
		activeId,
		activeSession,
		connectionTabs,
		activeTabId,
		activeTab,
		currentDatabaseName,
		catalogError,
		modelError,
		modelLoading,
		activeModel,
		structure = null,
		structureLoading = false,
		structureError = null,
		structureFetchedAt = null,
		onopentable,
		gridTable,
		tableLoading,
		rowSelectionIndex,
		rowSyncTick,
		tablePreviewTick = 0,
		baselineEpoch,
		inspectorData,
		onhistory,
		onbaselinechange,
		onselectrow,
		onopenmodelretry,
		onopenstructureretry,
		oninspectorchange,
		onsavequery,
		querysaving = false,
		onclosetab,
		onclosemany,
		onreloadtable,
		onreconnect,
		onopenreference,
		onopenstructure,
		objectKindById = {},
	}: Props = $props()

	let gridOriginals = $state<Record<string, unknown>[]>([])

	const tabPreview = $derived(
		activeTab?.kind === 'table' ? (activeTab.preview ?? null) : null,
	)
	const seedFilters = $derived(
		tabPreview?.filters.length ? tabPreview.filters : null,
	)
	const seedCodeFilter = $derived(tabPreview?.codeFilter ?? null)
	const seedSort = $derived(tabPreview?.sort ?? null)
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
							{onhistory}
							onsave={onsavequery}
							{onreconnect}
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
							onclick={onopenmodelretry}
						>
							Retry
						</button>
					</div>
				{/if}
				{#if modelLoading && !activeModel}
					<div class="flex flex-1 items-center justify-center p-8">
						<EmptyState
							title="Building diagram…"
							description="Reading tables, columns, and foreign keys from the live database."
						/>
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
							onrefresh={onopenmodelretry}
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
					engineSessionId={activeSession.engineSessionId}
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
					onretry={onopenstructureretry}
					{onopentable}
				/>
			{/key}
		{:else if activeSession && activeTab?.kind === 'table'}
			<div class="relative flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash">
				{#if catalogError}
					<div
						class="shrink-0 border-b border-line bg-danger/10 px-3 py-2 text-caption text-danger"
					>
						{catalogError}
						{#if onreloadtable}
							<button
								type="button"
								class="ml-2 border-none bg-transparent text-accent underline"
								onclick={() => void onreloadtable()}
							>
								Retry
							</button>
						{/if}
					</div>
				{/if}
				{#if tableLoading && (!gridTable || gridTable.columns.length === 0)}
					<div class="flex flex-1 items-center justify-center p-8">
						<EmptyState
							title="Loading table…"
							description="Reconnecting if needed, then fetching rows."
						/>
					</div>
				{:else if gridTable}
					<!-- Key on the tab, not the seed. Including seedFilters remounted
					     the grid on every filter apply and marked every cell unsaved.
					     Tab id keeps filter $state from leaking across tables. -->
					{#key activeTab.id}
						<DataGrid
							table={gridTable}
							selectedRowIndex={rowSelectionIndex}
							{rowSyncTick}
							{tablePreviewTick}
							{onbaselinechange}
							{onselectrow}
							engine={String(activeSession.profile.engine)}
							sessionId={activeSession.engineSessionId}
							profileReadOnly={!!activeSession.profile.readOnly}
							onreload={onreloadtable}
							{onreconnect}
							{seedFilters}
							{seedCodeFilter}
							{seedSort}
							codeFilterPlaceholder={resolveCapabilities(
								String(activeSession.profile.engine),
								activeSession.capabilities,
							).codeFilterPlaceholder}
							{onopenreference}
							onopenstructure={onopenstructure
								? () => onopenstructure(gridTable)
								: undefined}
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
								? `${catalogError} Use Retry, or pick another table.`
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
						onchange={oninspectorchange}
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
