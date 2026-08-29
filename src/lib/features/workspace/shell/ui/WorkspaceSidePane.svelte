<script lang="ts">
	import type {
		ConnectedSession,
		DatabaseMeta,
		TableMeta,
	} from '$core/contracts'
	import { Icon, IconButton, Resizable } from '$core/ui'
	import { ObjectExplorer } from '$features/workspace/explorer'
	import type {
		QueryHistoryEntry,
		SavedQueryListItem,
	} from '$features/workspace/query'
	import { QueryListPanel } from '$features/workspace/query'
	import {
		appPrefs,
		EXPLORER_PANEL_WIDTH_DEFAULT,
		EXPLORER_PANEL_WIDTH_MAX,
		EXPLORER_PANEL_WIDTH_MIN,
	} from '$lib/app'
	import ConnectionInfoModal from './ConnectionInfoModal.svelte'
	import type { SideNavMode } from './SessionsRail.svelte'

	interface Props {
		sideMode: SideNavMode
		activeSession: ConnectedSession | null
		databases: DatabaseMeta[]
		currentDatabaseName: string
		activeTableId: string | null
		refreshing: boolean
		queryHistory: QueryHistoryEntry[]
		savedQueries: SavedQueryListItem[]
		selectedQueryId: string | null
		selectedHistoryId: string | null
		onselectquery: (id: string) => void
		onselecthistory: (id: string) => void
		onselectdatabase: (name: string) => void
		onrefresh: () => void
		onrefreshtables?: () => void
		refreshingTables?: boolean
		oncreatedatabase: (name: string) => void
		oncreategroup?: (name: string) => void
		oncreateobject?: (name: string, group?: string) => void
		createBusy?: boolean
		createError?: string | null
		onopen: (table: TableMeta) => void
		ontableaction?: (
			action:
				| 'open'
				| 'erd'
				| 'structure'
				| 'rename'
				| 'drop'
				| 'truncate'
				| 'duplicate'
				| 'create'
				| 'export'
				| 'import',
			table: TableMeta,
			extra?: string,
		) => void
		onsavedaction?: (
			action: 'duplicate' | 'rename' | 'delete',
			id: string,
		) => void
	}

	let {
		sideMode,
		activeSession,
		databases,
		currentDatabaseName,
		activeTableId,
		refreshing,
		queryHistory,
		savedQueries,
		selectedQueryId,
		selectedHistoryId,
		onselectquery,
		onselecthistory,
		onselectdatabase,
		onrefresh,
		onrefreshtables,
		refreshingTables = false,
		oncreatedatabase,
		oncreategroup,
		oncreateobject,
		createBusy = false,
		createError = null,
		onopen,
		ontableaction,
		onsavedaction,
	}: Props = $props()

	const widthPx = $derived(appPrefs.explorerPanelWidth)
	let infoOpen = $state(false)
	const connectionName = $derived(activeSession?.profile.name?.trim() || '')
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
					onselect={onselectquery}
					onsavedaction={(action, id) => {
						if (action === 'open') onselectquery(id)
						else onsavedaction?.(action, id)
					}}
				/>
			{:else if sideMode === 'history'}
				<QueryListPanel
					mode="history"
					{savedQueries}
					history={queryHistory}
					selectedId={selectedHistoryId}
					onselect={onselecthistory}
				/>
			{:else if activeSession}
				<ObjectExplorer
					engine={String(activeSession.profile.engine)}
					{databases}
					selectedDatabase={currentDatabaseName}
					{activeTableId}
					{refreshing}
					{onselectdatabase}
					{onrefresh}
					{onrefreshtables}
					{refreshingTables}
					{oncreatedatabase}
					{oncreategroup}
					{oncreateobject}
					{createBusy}
					{createError}
					{onopen}
					{ontableaction}
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
