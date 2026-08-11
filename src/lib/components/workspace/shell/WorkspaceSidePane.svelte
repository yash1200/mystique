<script lang="ts">
	import type { QueryHistoryEntry, SavedQuery } from '$lib/data/mock-queries'
	import type { ConnectedSession } from '$lib/stores/workspace-session.svelte'
	import type { DatabaseMeta, TableMeta } from '$lib/types/workspace'
	import ObjectExplorer from '../ObjectExplorer.svelte'
	import QueryListPanel from '../QueryListPanel.svelte'
	import type { SideNavMode } from '../SessionsRail.svelte'

	interface Props {
		sideMode: SideNavMode
		activeSession: ConnectedSession | null
		databases: DatabaseMeta[]
		currentDatabaseName: string
		activeTableId: string | null
		refreshing: boolean
		modelLoading: boolean
		queryHistory: QueryHistoryEntry[]
		savedQueries: SavedQuery[]
		selectedQueryId: string | null
		selectedHistoryId: string | null
		onselectquery: (id: string) => void
		onselecthistory: (id: string) => void
		onselectdatabase: (name: string) => void
		onrefresh: () => void
		oncreatedatabase: (name: string) => void
		onopen: (table: TableMeta) => void
		onopenmodel: () => void
		ontableaction?: (
			action:
				| 'open'
				| 'erd'
				| 'rename'
				| 'drop'
				| 'truncate'
				| 'duplicate'
				| 'create',
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
		modelLoading,
		queryHistory,
		savedQueries,
		selectedQueryId,
		selectedHistoryId,
		onselectquery,
		onselecthistory,
		onselectdatabase,
		onrefresh,
		oncreatedatabase,
		onopen,
		onopenmodel,
		ontableaction,
		onsavedaction,
	}: Props = $props()
</script>

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
		{modelLoading}
		{onselectdatabase}
		{onrefresh}
		{oncreatedatabase}
		{onopen}
		{onopenmodel}
		{ontableaction}
		readOnly={!!activeSession.profile.readOnly}
		capabilities={activeSession.capabilities}
	/>
{:else}
	<aside
		class="flex w-60 shrink-0 flex-col items-center justify-center border-r border-line bg-panel px-4 text-center"
	>
		<p class="text-caption text-fg-muted">Select a session</p>
	</aside>
{/if}
