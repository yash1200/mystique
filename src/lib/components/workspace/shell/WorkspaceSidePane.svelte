<script lang="ts">
	import type { QueryHistoryEntry, SavedQuery } from '$lib/data/mock-queries'
	import type { ConnectedSession } from '$lib/stores/workspace-session.svelte'
	import type { DatabaseMeta, TableMeta } from '$lib/types/workspace'
	import {
		clampExplorerPanelWidth,
		EXPLORER_PANEL_WIDTH_DEFAULT,
		readExplorerPanelWidth,
		writeExplorerPanelWidth,
	} from '$lib/utils/appPrefs'
	import { cn } from '$lib/utils/cn'
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
		onrefreshtables?: () => void
		refreshingTables?: boolean
		oncreatedatabase: (name: string) => void
		oncreategroup?: (name: string) => void
		oncreateobject?: (name: string, group?: string) => void
		createBusy?: boolean
		createError?: string | null
		onopen: (table: TableMeta) => void
		onopenmodel: () => void
		ontableaction?: (
			action:
				| 'open'
				| 'erd'
				| 'structure'
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
		onrefreshtables,
		refreshingTables = false,
		oncreatedatabase,
		oncreategroup,
		oncreateobject,
		createBusy = false,
		createError = null,
		onopen,
		onopenmodel,
		ontableaction,
		onsavedaction,
	}: Props = $props()

	let widthPx = $state(readExplorerPanelWidth())
	let dragging = $state(false)

	function setWidth(next: number, persist = false) {
		widthPx = clampExplorerPanelWidth(next)
		if (persist) writeExplorerPanelWidth(widthPx)
	}

	function onResizePointerDown(e: PointerEvent) {
		if (e.button !== 0) return
		e.preventDefault()
		const startX = e.clientX
		const startW = widthPx
		dragging = true
		const target = e.currentTarget as HTMLElement
		target.setPointerCapture(e.pointerId)
		document.body.style.cursor = 'col-resize'
		document.body.style.userSelect = 'none'

		const onMove = (ev: PointerEvent) => {
			setWidth(startW + (ev.clientX - startX))
		}
		const onUp = (ev: PointerEvent) => {
			dragging = false
			document.body.style.cursor = ''
			document.body.style.userSelect = ''
			try {
				target.releasePointerCapture(ev.pointerId)
			} catch {
				/* ignore */
			}
			target.removeEventListener('pointermove', onMove)
			target.removeEventListener('pointerup', onUp)
			target.removeEventListener('pointercancel', onUp)
			writeExplorerPanelWidth(widthPx)
		}
		target.addEventListener('pointermove', onMove)
		target.addEventListener('pointerup', onUp)
		target.addEventListener('pointercancel', onUp)
	}

	function onResizeDblClick() {
		setWidth(EXPLORER_PANEL_WIDTH_DEFAULT, true)
	}
</script>

<div
	class="relative flex h-full shrink-0 border-r border-line bg-panel"
	style="width: {widthPx}px"
	data-explorer-panel
>
	<div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
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
				{onrefreshtables}
				{refreshingTables}
				{oncreatedatabase}
				{oncreategroup}
				{oncreateobject}
				{createBusy}
				{createError}
				{onopen}
				{onopenmodel}
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

	<button
		type="button"
		aria-label="Resize side panel ({widthPx}px)"
		title="Drag to resize · double-click to reset"
		class={cn(
			'group absolute top-0 right-0 z-10 flex h-full w-2 shrink-0 translate-x-1/2 cursor-col-resize items-center justify-center border-none bg-transparent p-0',
			'hover:bg-hover/80',
			dragging && 'bg-accent-muted',
		)}
		onpointerdown={onResizePointerDown}
		ondblclick={onResizeDblClick}
		onkeydown={(e) => {
			if (e.key === 'ArrowLeft') {
				e.preventDefault()
				setWidth(widthPx - 12, true)
			} else if (e.key === 'ArrowRight') {
				e.preventDefault()
				setWidth(widthPx + 12, true)
			} else if (e.key === 'Home') {
				e.preventDefault()
				setWidth(EXPLORER_PANEL_WIDTH_DEFAULT, true)
			}
		}}
	>
		<span
			class={cn(
				'pointer-events-none h-10 w-0.5 rounded-full bg-line-strong opacity-0 transition-opacity',
				'group-hover:opacity-100 group-focus-visible:opacity-100',
				dragging && 'bg-accent opacity-100',
			)}
			aria-hidden="true"
		></span>
	</button>
</div>
