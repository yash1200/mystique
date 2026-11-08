<script lang="ts">
	import { commands } from '#core/commands'
	import { Icon, IconButton, Resizable } from '#core/ui'
	import {
		catalogStore,
		createCatalogGroup,
		createCatalogObject,
		createDatabase,
		databaseNameFor,
		refreshCurrentTables,
		refreshDatabases,
		selectDatabase,
	} from '#features/workspace/catalog'
	import {
		ObjectExplorer,
		onTableExplorerAction,
	} from '#features/workspace/explorer'
	import { tablePreview } from '#features/workspace/grid'
	import {
		clearHistorySearch,
		loadOlderHistory,
		queryLibrary,
		QueryListPanel,
		refreshHistory,
		refreshSavedQueries,
		selectHistory,
		selectSavedQuery,
		setHistorySearch,
	} from '#features/workspace/query'
	import {
		appPrefs,
		cmd,
		EXPLORER_PANEL_WIDTH_DEFAULT,
		EXPLORER_PANEL_WIDTH_MAX,
		EXPLORER_PANEL_WIDTH_MIN,
	} from '#lib/app'
	import { onMount, untrack } from 'svelte'
	import { resolveCapabilities } from '#engines'
	import {
		liveStatusMark,
		workspaceSession,
		workspaceStudio,
	} from '#features/workspace/session'
	import ConnectionInfoModal from './ConnectionInfoModal.svelte'

	const sideMode = $derived(workspaceStudio.sideMode)
	const activeSession = $derived(workspaceSession.activeSession)
	const activeId = $derived(workspaceSession.activeConnectionId)
	const activeTab = $derived(workspaceSession.activeTab)
	const databases = $derived(catalogStore.databases(activeId))
	const currentDatabaseName = $derived(databaseNameFor(activeId))
	const activeTableId = $derived(
		activeTab &&
			(activeTab.kind === 'table' || activeTab.kind === 'structure') &&
			activeTab.connectionId === activeId
			? (activeTab.tableId ?? null)
			: null,
	)
	const catalogStatus = $derived(catalogStore.statusOf(activeId))
	const refreshing = $derived(catalogStatus.refreshing)
	const refreshingTables = $derived(catalogStatus.refreshingTables)
	const createBusy = $derived(catalogStatus.createBusy)
	const queryHistory = $derived(workspaceStudio.queryHistory)
	const savedQueries = $derived(workspaceStudio.savedQueries)
	const selectedQueryId = $derived(workspaceStudio.selectedQueryId)
	const selectedHistoryId = $derived(workspaceStudio.selectedHistoryId)

	const widthPx = $derived(appPrefs.explorerPanelWidth)
	let infoOpen = $state(false)
	const connectionName = $derived(activeSession?.profile.name?.trim() || '')
	const liveMark = $derived(liveStatusMark(activeSession?.liveStatus))
	const explorerOk = $derived(
		!activeSession ||
			resolveCapabilities(
				String(activeSession.profile.engine),
				activeSession.capabilities,
			).supportsExplorer,
	)

	onMount(() =>
		commands.register({
			...cmd.connectionDetails,
			when: () => workspaceSession.activeSession != null,
			run: () => (infoOpen = true),
		}),
	)

	$effect(() => {
		void refreshSavedQueries(activeId)
	})

	$effect(() => {
		const mode = sideMode
		const id = activeId
		untrack(() => {
			if (mode === 'history') {
				clearHistorySearch()
				void refreshHistory({ preferSelectLatest: true, force: true })
			} else if (id) {
				void refreshHistory()
			}
		})
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
				{#key activeId}
				<QueryListPanel
					mode="history"
					{savedQueries}
					history={queryHistory}
					selectedId={selectedHistoryId}
					onselect={selectHistory}
					onrefresh={() => void refreshHistory({ force: true })}
					onhistorysearch={setHistorySearch}
					onhistorymore={() => void loadOlderHistory()}
					historyHasMore={workspaceStudio.historyHasMore}
					historyLoadingMore={workspaceStudio.historyLoadingMore}
					loading={workspaceStudio.historyLoading}
				/>
				{/key}
			{:else if activeSession && explorerOk}
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
			{:else if activeSession}
				<aside
					class="flex h-full w-full flex-col items-center justify-center px-4 text-center"
					aria-label="Catalog unavailable"
				>
					<p class="text-caption text-fg-muted">
						Catalog is not available for this connection.
					</p>
				</aside>
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
				<span
					class="grid size-control-sm shrink-0 place-items-center {liveMark.class}"
					title={liveMark.label}
					aria-label={liveMark.label}
					role="status"
				>
					<Icon name={liveMark.icon} class="size-4" />
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
