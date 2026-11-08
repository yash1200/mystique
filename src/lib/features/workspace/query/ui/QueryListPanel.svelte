<script lang="ts">
	import {
		ContextMenu,
		Icon,
		IconButton,
		Input,
		type ContextMenuItem,
	} from '#core/ui'
	import { appPrefs, type SavedQueryListSort } from '#lib/app'
	import type { QueryHistoryEntry } from '../history-types'
	import type { SavedQueryListItem } from '../saved-types'
	import HistoryQueryList from './HistoryQueryList.svelte'
	import SavedQueryList, {
		type SavedQueryMenuAction,
	} from './SavedQueryList.svelte'

	type Mode = 'saved-queries' | 'history'

	interface Props {
		mode: Mode
		savedQueries: SavedQueryListItem[]
		history: QueryHistoryEntry[]
		selectedId: string | null
		loading?: boolean
		onselect: (id: string) => void
		onsavedaction?: (action: SavedQueryMenuAction, id: string) => void
		onrefresh: () => void | Promise<void>
		onhistorysearch?: (query: string) => void
		onhistorymore?: () => void
		historyHasMore?: boolean
		historyLoadingMore?: boolean
	}

	let {
		mode,
		savedQueries,
		history,
		selectedId,
		onselect,
		onsavedaction,
		onrefresh,
		onhistorysearch,
		onhistorymore,
		historyHasMore = false,
		historyLoadingMore = false,
		loading = false,
	}: Props = $props()


	const SORT_OPTIONS: { id: SavedQueryListSort; label: string }[] = [
		{ id: 'name', label: 'Name' },
		{ id: 'created', label: 'Created' },
		{ id: 'recent', label: 'Updated' },
	]

	let query = $state('')
	let sortMenu = $state<{ x: number; y: number } | null>(null)
	let refreshBusy = $state(false)

	const title = $derived(mode === 'saved-queries' ? 'Saved queries' : 'History')

	function byName(a: SavedQueryListItem, b: SavedQueryListItem): number {
		return (
			a.name.localeCompare(b.name, undefined, {
				numeric: true,
				sensitivity: 'base',
			}) || a.id.localeCompare(b.id)
		)
	}

	function compareSaved(
		a: SavedQueryListItem,
		b: SavedQueryListItem,
		sort: SavedQueryListSort,
	): number {
		if (a.favorite !== b.favorite) return a.favorite ? -1 : 1
		let n = 0
		if (sort === 'name') {
			n = byName(a, b)
		} else if (sort === 'created') {
			const ta = Date.parse(a.createdAt)
			const tb = Date.parse(b.createdAt)
			n = (Number.isFinite(tb) ? tb : 0) - (Number.isFinite(ta) ? ta : 0)
		} else {
			const ta = Date.parse(a.updatedAt)
			const tb = Date.parse(b.updatedAt)
			n = (Number.isFinite(tb) ? tb : 0) - (Number.isFinite(ta) ? ta : 0)
		}
		if (n !== 0) return n
		return byName(a, b)
	}

	const filteredSaved = $derived.by(() => {
		const q = query.trim().toLowerCase()
		const rows = !q
			? savedQueries
			: savedQueries.filter(
					(s) =>
						s.name.toLowerCase().includes(q) || s.sql.toLowerCase().includes(q),
				)
		return [...rows].sort((a, b) =>
			compareSaved(a, b, appPrefs.savedQueryListSort),
		)
	})

	const filterId = $derived(
		mode === 'history' ? 'history-search' : 'saved-query-filter',
	)
	const filterLabel = $derived(
		mode === 'history' ? 'Search history' : 'Filter saved queries',
	)
	const filterPlaceholder = $derived(
		mode === 'history' ? 'Search queries…' : 'Filter…',
	)

	$effect(() => {
		if (mode !== 'history') return
		const q = query
		const handle = setTimeout(() => onhistorysearch?.(q), 250)
		return () => clearTimeout(handle)
	})

	const sortItems = $derived.by((): ContextMenuItem[] =>
		SORT_OPTIONS.map((opt) => ({
			id: opt.id,
			label: opt.label,
			checked: appPrefs.savedQueryListSort === opt.id,
		})),
	)

	function toggleSort(e: MouseEvent) {
		if (sortMenu) {
			sortMenu = null
			return
		}
		const el = e.currentTarget
		if (!(el instanceof HTMLElement)) return
		const rect = el.getBoundingClientRect()
		sortMenu = { x: rect.right, y: rect.bottom }
	}

	function setSort(id: string) {
		if (id === 'name' || id === 'created' || id === 'recent') {
			appPrefs.setSavedQueryListSort(id)
		}
		sortMenu = null
	}

	async function refresh() {
		if (refreshBusy) return
		refreshBusy = true
		try {
			await onrefresh()
		} finally {
			refreshBusy = false
		}
	}
</script>

<aside class="flex h-full w-full min-w-0 flex-col bg-panel" aria-label={title}>
	<header class="shrink-0 border-b border-line">
		<div class="flex items-center gap-1 px-3 pt-2.5 pb-1.5">
			<h2 class="m-0 min-w-0 flex-1 truncate text-body font-semibold text-fg">
				{title}
			</h2>
			<div
				class="flex shrink-0 items-center gap-0.5"
				role="group"
				aria-label="{title} actions"
			>
				<IconButton
					label={mode === 'saved-queries'
						? 'Refresh queries'
						: 'Refresh history'}
					variant="quiet"
					size="sm"
					disabled={refreshBusy}
					aria-busy={refreshBusy}
					class={refreshBusy ? 'opacity-45' : ''}
					onclick={() => void refresh()}
				>
					<Icon name="refresh" class={refreshBusy ? 'animate-spin' : ''} />
				</IconButton>
				{#if mode === 'saved-queries'}
					<div class="relative">
						<IconButton
							label="Order queries"
							variant="quiet"
							size="sm"
							active={sortMenu != null}
							onclick={toggleSort}
							aria-haspopup="menu"
							aria-expanded={sortMenu != null}
						>
							<Icon name="sort" />
						</IconButton>
						{#if sortMenu}
							<ContextMenu
								x={sortMenu.x}
								y={sortMenu.y}
								items={sortItems}
								ariaLabel="Order queries"
								onpick={setSort}
								onclose={() => (sortMenu = null)}
							/>
						{/if}
					</div>
				{/if}
			</div>
		</div>
		<div class="px-2 pb-2">
			<div class="relative">
				<label class="sr-only" for={filterId}>{filterLabel}</label>
				<Icon
					name="search"
					class="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-fg-muted"
				/>
				<Input
					id={filterId}
					type="search"
					placeholder={filterPlaceholder}
					bind:value={query}
					inputSize="sm"
					inset={query.length > 0 ? 'both' : 'start'}
					class="[appearance:textfield] [&::-webkit-search-cancel-button]:hidden [&::-webkit-search-decoration]:hidden"
					onkeydown={(e) => {
						if (e.key !== 'Escape' || !query) return
						e.stopPropagation()
						query = ''
					}}
				/>
				{#if query.length > 0}
					<button
						type="button"
						class="absolute top-1/2 right-0.5 grid size-6 -translate-y-1/2 place-items-center rounded-sm border-none bg-transparent text-fg-muted transition-colors duration-150 hover:bg-hover hover:text-fg focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1 enabled:active:scale-[0.97]"
						aria-label="Clear filter"
						onclick={() => (query = '')}
					>
						<Icon name="close" class="size-3" />
					</button>
				{/if}
			</div>
		</div>
	</header>

	<div class="flex-1 overflow-auto px-1.5 py-2">
		{#if loading && !query.trim() && (mode === 'saved-queries' ? savedQueries.length === 0 : history.length === 0)}
			<p class="m-2 text-caption leading-relaxed text-fg-muted">
				Loading…
			</p>
		{:else if mode === 'saved-queries'}
			<SavedQueryList
				items={filteredSaved}
				{query}
				{selectedId}
				{onselect}
				onaction={(action, id) => onsavedaction?.(action, id)}
			/>
		{:else}
			<HistoryQueryList
				items={history}
				{query}
				{selectedId}
				{onselect}
				hasMore={historyHasMore}
				loadingMore={historyLoadingMore}
				onmore={() => onhistorymore?.()}
			/>
		{/if}
	</div>
</aside>
