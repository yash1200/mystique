<script lang="ts">
	import { Icon, IconButton, Resizable } from '$core/ui'
	import {
		createCatalogGroup,
		createCatalogObject,
		createDatabase,
		refreshCurrentTables,
		refreshDatabases,
		selectDatabase,
	} from '$features/workspace/catalog'
	import {
		ObjectExplorer,
		onTableExplorerAction,
	} from '$features/workspace/explorer'
	import { tablePreview } from '$features/workspace/grid'
	import {
		queryLibrary,
		QueryListPanel,
		refreshHistory,
		refreshSavedQueries,
		selectHistory,
		selectSavedQuery,
	} from '$features/workspace/query'
	import {
		appPrefs,
		EXPLORER_PANEL_WIDTH_DEFAULT,
		EXPLORER_PANEL_WIDTH_MAX,
		EXPLORER_PANEL_WIDTH_MIN,
	} from '$lib/app'
	import { catalogsFor, selectedDatabaseName } from '../lookup'
	import { workspaceSession } from '../session.svelte'
	import { workspaceStudio } from '../studio.svelte'
	import ConnectionInfoModal from './ConnectionInfoModal.svelte'

	const sideMode = $derived(workspaceStudio.sideMode)
	const activeSession = $derived(workspaceSession.activeSession)
	const activeId = $derived(workspaceSession.activeConnectionId)
	const activeTab = $derived(workspaceSession.activeTab)
	const databases = $derived(catalogsFor(workspaceStudio.catalogs, activeId))
	const currentDatabaseName = $derived(
		selectedDatabaseName(workspaceStudio.selectedDb, activeId, databases),
	)
	const activeTableId = $derived(
		activeTab &&
			(activeTab.kind === 'table' || activeTab.kind === 'structure') &&
			activeTab.connectionId === activeId
			? (activeTab.tableId ?? null)
			: null,
	)
	const refreshing = $derived(workspaceStudio.refreshing)
	const refreshingTables = $derived(workspaceStudio.refreshingTables)
	const createBusy = $derived(workspaceStudio.createBusy)
	const queryHistory = $derived(workspaceStudio.queryHistory)
	const savedQueries = $derived(workspaceStudio.savedQueries)
	const selectedQueryId = $derived(workspaceStudio.selectedQueryId)
	const selectedHistoryId = $derived(workspaceStudio.selectedHistoryId)

	const widthPx = $derived(appPrefs.explorerPanelWidth)
	let infoOpen = $state(false)
	const connectionName = $derived(activeSession?.profile.name?.trim() || '')

	$effect(() => {
		void refreshSavedQueries(activeId)
	})

	$effect(() => {
		const mode = sideMode
		if (mode === 'history' || activeId) {
			void refreshHistory({ preferSelectLatest: mode === 'history' })
		}
	})
</script>

<Resizable
	size={widthPx}
	defaultSize={EXPLORER_PANEL_WIDTH_DEFAULT}
	min={EXPLORER_PANEL_WIDTH_MIN}
	max={EXPLORER_PANEL_WIDTH_MAX}
	label="Resize side panel"
	class="h-full border-r border-line bg-panel"
	data-explorer-panel
	onresize={(n) => appPrefs.setExplorerPanelWidth(n, false)}
	oncommit={(n) => appPrefs.setExplorerPanelWidth(n, true)}
>
	<div class="flex h-full min-h-0 flex-col">
		<div class="min-h-0 flex-1 overflow-hidden">
			{#if sideMode === 'saved-queries'}
				<QueryListPanel
					mode="saved-queries"
					{savedQueries}
					history={queryHistory}
					selectedId={selectedQueryId}
					onselect={selectSavedQuery}
					onsavedaction={(action, id) => {
						if (action === 'open') selectSavedQuery(id)
						else void queryLibrary.action(action, id)
					}}
					onrefresh={() => void refreshSavedQueries(activeId, true)}
				/>
			{:else if sideMode === 'history'}
				<QueryListPanel
					mode="history"
					{savedQueries}
					history={queryHistory}
					selectedId={selectedHistoryId}
					onselect={selectHistory}
					onrefresh={() => void refreshHistory({ force: true })}
				/>
			{:else if activeSession}
				<ObjectExplorer
					engine={String(activeSession.profile.engine)}
					{databases}
					selectedDatabase={currentDatabaseName}
					{activeTableId}
					{refreshing}
					onselectdatabase={(name) => void selectDatabase(name)}
					onrefresh={() => void refreshDatabases()}
					onrefreshtables={() => void refreshCurrentTables()}
					{refreshingTables}
					oncreatedatabase={createDatabase}
					oncreategroup={(name) => void createCatalogGroup(name)}
					oncreateobject={(name, group) =>
						void createCatalogObject(name, group)}
					{createBusy}
					createError={null}
					onopen={(table) => void tablePreview.open(table)}
					ontableaction={(action, table, extra) =>
						void onTableExplorerAction(action, table, extra)}
					readOnly={!!activeSession.profile.readOnly}
					capabilities={activeSession.capabilities}
				/>
			{:else}
				<aside
					class="flex h-full w-full flex-col items-center justify-center px-4 text-center"
					aria-label="No session"
				>
					<p class="text-caption text-fg-muted">Select a session</p>
				</aside>
			{/if}
		</div>
		{#if activeSession}
			<footer
				class="relative flex h-statusbar shrink-0 items-center gap-1.5 overflow-hidden border-t border-(--connection-color)/35 px-2 pl-3 conn-tint"
				aria-label="Connection"
			>
				<span
					class="absolute inset-y-0 left-0 w-stripe bg-conn"
					aria-hidden="true"
				></span>
				<span
					class="min-w-0 flex-1 truncate text-caption font-medium text-fg"
					title={connectionName}
				>
					{connectionName}
				</span>
				<IconButton
					label="Connection info"
					size="sm"
					variant="quiet"
					class="hover:bg-(--connection-color)/12 hover:text-(--connection-color)"
					onclick={() => (infoOpen = true)}
				>
					<Icon name="info" />
				</IconButton>
			</footer>
		{/if}
	</div>
	<ConnectionInfoModal
		open={infoOpen && !!activeSession}
		profile={activeSession?.profile ?? null}
		onclose={() => (infoOpen = false)}
	/>
</Resizable>
