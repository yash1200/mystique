<script lang="ts">
	import type { QueryHistoryItem } from '$lib/api/history'
	import { familyForEngine } from '$lib/api/liveEr'
	import ErWorkspace from '../ErWorkspace.svelte'
	import { EmptyState } from '$lib/components/ui'
	import type { ConnectedSession } from '$lib/stores/workspace-session.svelte'
	import { workspaceSession } from '$lib/stores/workspace-session.svelte'
	import type { ErDiagramState } from '$lib/types/er'
	import type { TableMeta, WorkspaceTab } from '$lib/types/workspace'
	import DataGrid from '../DataGrid.svelte'
	import QueryCanvas from '../QueryCanvas.svelte'
	import RowInspector from '../RowInspector.svelte'
	import type { SideNavMode } from '../SessionsRail.svelte'
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
		gridTable: TableMeta | null
		tableLoading: boolean
		rowSelectionIndex: number | null
		rowSyncTick: number
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
		oninspectorchange: (column: string, value: unknown) => void
		onsavequery?: () => void
		querysaving?: boolean
		/** Close request (may prompt if dirty). */
		onclosetab?: (id: string) => void
		onreloadtable?: (opts?: { offset?: number }) => Promise<void>
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
		gridTable,
		tableLoading,
		rowSelectionIndex,
		rowSyncTick,
		baselineEpoch,
		inspectorData,
		onhistory,
		onbaselinechange,
		onselectrow,
		onopenmodelretry,
		oninspectorchange,
		onsavequery,
		querysaving = false,
		onclosetab,
		onreloadtable,
	}: Props = $props()
</script>

<main class="flex min-w-0 flex-1 flex-col">
	<!-- Tabs stay visible: explorer / saved / history all open content in the editor. -->
	{#if (sideMode === 'explorer' || sideMode === 'saved-queries' || sideMode === 'history') && activeId}
		<TableTabs
			tabs={connectionTabs}
			{activeTabId}
			onactivate={(id) => workspaceSession.setActiveTab(id)}
			onclose={(id) =>
				onclosetab ? onclosetab(id) : workspaceSession.closeTab(id)}
			onopenquery={() => workspaceSession.openNewQuery(activeId)}
		/>
	{/if}

	<div class="flex min-h-0 flex-1">
		{#if activeSession && activeTab?.kind === 'query'}
			{#key activeTab.id}
				<QueryCanvas
					connectionId={activeSession.profile.id}
					connectionName={activeSession.profile.name}
					engine={String(activeSession.profile.engine)}
					profileReadOnly={!!activeSession.profile.readOnly}
					engineSessionId={activeSession.engineSessionId}
					database={currentDatabaseName}
					text={workspaceSession.queryDrafts[activeTab.id] ?? ''}
					writeEnabled={workspaceSession.queryWriteMode[activeTab.id] ?? false}
					dirty={!!activeTab.dirty}
					saving={querysaving}
					ontextchange={(v) => workspaceSession.setQueryDraft(activeTab.id, v)}
					onwritechange={(v) =>
						workspaceSession.setQueryWriteMode(activeTab.id, v)}
					{onhistory}
					onsave={onsavequery}
				/>
			{/key}
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
								familyForEngine(String(activeSession.profile.engine))}
							initialDiagram={activeModel.diagram}
							initialName={activeModel.title}
							statusNote={activeModel.summary}
							embedded={true}
						/>
					{/key}
				{:else}
					<div class="flex flex-1 items-center justify-center p-8">
						<EmptyState
							title="ER diagram"
							description="Click “ER diagram” in the explorer to build a live schema map for this database."
						/>
					</div>
				{/if}
			</div>
		{:else if activeSession && activeTab?.kind === 'table'}
			<div class="relative flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash">
				{#if catalogError}
					<div
						class="shrink-0 border-b border-line bg-danger/10 px-3 py-2 text-caption text-danger"
					>
						{catalogError}
					</div>
				{/if}
				{#if gridTable}
					{#key gridTable.id}
						<DataGrid
							table={gridTable}
							selectedRowIndex={rowSelectionIndex}
							{rowSyncTick}
							{onbaselinechange}
							{onselectrow}
							engine={String(activeSession.profile.engine)}
							sessionId={activeSession.engineSessionId}
							profileReadOnly={!!activeSession.profile.readOnly}
							onreload={onreloadtable}
							ondirtychange={(dirty) =>
								workspaceSession.setTabDirty(activeTab.id, dirty)}
						/>
					{/key}
				{:else}
					<div class="flex flex-1 items-center justify-center p-8">
						<EmptyState title="Table" description="Loading table metadata…" />
					</div>
				{/if}
			</div>
			{#if inspectorData}
				<RowInspector
					tableTitle={inspectorData.tableTitle}
					columns={inspectorData.columns}
					row={inspectorData.row}
					rowIndex={inspectorData.rowIndex}
					{baselineEpoch}
					onclose={() => workspaceSession.clearRowSelection()}
					onchange={oninspectorchange}
				/>
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
