<script lang="ts">
	import type { ObjectStructure, TablePreviewFilter } from '$lib/api/engines'
	import type { QueryHistoryItem } from '$lib/api/history'
	import { EmptyState } from '$lib/components/core'
	import { appPrefs } from '$lib/stores/app-prefs.svelte'
	import type { ConnectedSession } from '$lib/stores/workspace-session.svelte'
	import { workspaceSession } from '$lib/stores/workspace-session.svelte'
	import { colorHex } from '$lib/types/connection'
	import { familyFromCaps, resolveCapabilities } from '$lib/types/engineCaps'
	import type { ErDiagramState } from '$lib/types/er'
	import type {
		CatalogObjectKind,
		ColumnReference,
		TableMeta,
		WorkspaceTab,
	} from '$lib/types/workspace'
	import DataGrid from '../DataGrid.svelte'
	import ErWorkspace from '../er_diagram/ErWorkspace.svelte'
	import AgentChatPane from '../query/AgentChatPane.svelte'
	import QueryCanvas from '../QueryCanvas.svelte'
	import RowInspector from '../RowInspector.svelte'
	import type { SideNavMode } from '../SessionsRail.svelte'
	import StructurePanel from '../StructurePanel.svelte'
	import TableTabs from '../TableTabs.svelte'
	import WorkspaceEmpty from '../WorkspaceEmpty.svelte'

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
		} | null
		structure?: ObjectStructure | null
		structureLoading?: boolean
		structureError?: string | null
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
		/** Seeded filters when jumping via FK (keeps pager/refresh consistent). */
		seedFilters?: TablePreviewFilter[] | null
		seedCodeFilter?: string | null
		/** Seeded preview sort so remounts keep the same column order. */
		seedSort?: { column: string; dir: 'asc' | 'desc' } | null
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
		seedFilters = null,
		seedCodeFilter = null,
		seedSort = null,
		onopenreference,
		onopenstructure,
		objectKindById = {},
	}: Props = $props()

	let gridOriginals = $state<Record<string, unknown>[]>([])
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
							embedded={true}
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
							description="Click the diagram action in the explorer to build a live schema map for this database."
						/>
					</div>
				{/if}
			</div>
		{:else if activeSession && activeTab?.kind === 'structure'}
			{#key activeTab.id}
				<StructurePanel
					{structure}
					loading={structureLoading}
					error={structureError}
					onretry={onopenstructureretry}
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
					<!-- Key on table id only. Including seedFilters remounted the
					     grid on every filter apply, which dropped the saved
					     baseline and marked every cell unsaved. -->
					{#key gridTable.id}
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
							onopenstructure={
								onopenstructure
									? () => onopenstructure(gridTable)
									: undefined
							}
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
