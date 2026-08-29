<script lang="ts">
	import { ContextMenu, Input, type ContextMenuItem } from '$core/ui'
	import { cn, formatRelativeTime } from '$core/util'
	import { CatalogIcon } from '$features/workspace/catalog'
	import { previewSql } from '../display'
	import type { QueryHistoryEntry } from '../history-types'
	import type { SavedQueryListItem } from '../saved-types'
	type Mode = 'saved-queries' | 'history'
	type SavedMenuAction = 'open' | 'duplicate' | 'rename' | 'delete'

	interface Props {
		mode: Mode
		savedQueries: SavedQueryListItem[]
		history: QueryHistoryEntry[]
		selectedId: string | null
		onselect: (id: string) => void
		/** Context menu actions for saved queries (open / duplicate / rename / delete). */
		onsavedaction?: (action: SavedMenuAction, id: string) => void
	}

	let {
		mode,
		savedQueries,
		history,
		selectedId,
		onselect,
		onsavedaction,
	}: Props = $props()

	let query = $state('')
	let menu = $state<{ id: string; x: number; y: number } | null>(null)

	const title = $derived(
		mode === 'saved-queries' ? 'Saved queries' : 'Query history',
	)
	const subtitle = $derived(
		mode === 'saved-queries'
			? 'Click to open · right-click for more'
			: 'Click to open in the editor',
	)

	const filteredSaved = $derived.by(() => {
		const q = query.trim().toLowerCase()
		if (!q) return savedQueries
		return savedQueries.filter(
			(s) =>
				s.name.toLowerCase().includes(q) || s.sql.toLowerCase().includes(q),
		)
	})

	const filteredHistory = $derived.by(() => {
		const q = query.trim().toLowerCase()
		if (!q) return history
		return history.filter(
			(h) =>
				h.sql.toLowerCase().includes(q) ||
				(h.connectionName ?? '').toLowerCase().includes(q) ||
				(h.error ?? '').toLowerCase().includes(q),
		)
	})

	const count = $derived(
		mode === 'saved-queries' ? filteredSaved.length : filteredHistory.length,
	)

	const menuItems = $derived.by((): ContextMenuItem[] => [
		{ id: 'open', label: 'Open' },
		{ id: 'duplicate', label: 'Duplicate' },
		{ id: 'rename', label: 'Rename' },
		{ id: 'delete', label: 'Delete', danger: true, separatorBefore: true },
	])

	function openMenu(e: MouseEvent, id: string) {
		e.preventDefault()
		e.stopPropagation()
		menu = { id, x: e.clientX, y: e.clientY }
	}

	function onMenuPick(action: string) {
		const id = menu?.id
		menu = null
		if (!id) return
		if (action === 'open') {
			onselect(id)
			return
		}
		onsavedaction?.(action as SavedMenuAction, id)
	}
</script>

<aside class="flex h-full w-full min-w-0 flex-col bg-panel" aria-label={title}>
	<header class="shrink-0 border-b border-line px-3 py-2.5">
		<div class="truncate text-label font-semibold text-fg">{title}</div>
		<div class="mt-0.5 truncate text-caption text-fg-muted">{subtitle}</div>
	</header>

	<div class="shrink-0 px-2 py-2">
		<Input
			type="search"
			placeholder={mode === 'saved-queries'
				? 'Filter saved queries…'
				: 'Filter history…'}
			bind:value={query}
			inputSize="sm"
		/>
	</div>

	<div class="flex-1 overflow-auto px-1.5 pb-2">
		{#if count === 0}
			<p class="m-2 text-caption leading-relaxed text-fg-muted">
				{#if query.trim()}
					No matches for “{query.trim()}”.
				{:else if mode === 'saved-queries'}
					No saved queries yet. Save a query from the editor.
				{:else}
					No query history yet.
				{/if}
			</p>
		{:else if mode === 'saved-queries'}
			<ul class="m-0 list-none p-0">
				{#each filteredSaved as item (item.id)}
					{@const active = selectedId === item.id}
					<li>
						<button
							type="button"
							class={cn(
								'flex w-full items-start gap-1.5 rounded-md border-none px-2 py-1.5 text-left transition-colors',
								active
									? 'bg-accent-muted text-accent'
									: 'bg-transparent text-fg hover:bg-hover',
							)}
							onclick={() => onselect(item.id)}
							oncontextmenu={(e) => openMenu(e, item.id)}
						>
							<span
								class={cn(
									'mt-0.5 grid size-5 shrink-0 place-items-center rounded-sm',
									active ? 'bg-accent/15' : 'bg-input',
								)}
								aria-hidden="true"
							>
								<CatalogIcon name="query" class="size-3" tone="accent" />
							</span>
							<span class="min-w-0 flex-1">
								<span class="block truncate text-label font-medium"
									>{item.name}</span
								>
								<span
									class={cn(
										'block text-caption',
										active ? 'text-accent/70' : 'text-fg-muted',
									)}
								>
									{formatRelativeTime(item.updatedAt)}
								</span>
							</span>
						</button>
					</li>
				{/each}
			</ul>
		{:else}
			<ul class="m-0 list-none p-0">
				{#each filteredHistory as item (item.id)}
					{@const active = selectedId === item.id}
					<li>
						<button
							type="button"
							class={cn(
								'flex w-full items-start gap-1.5 rounded-md border-none px-2 py-1.5 text-left transition-colors',
								active
									? 'bg-accent-muted text-accent'
									: 'bg-transparent text-fg hover:bg-hover',
							)}
							onclick={() => onselect(item.id)}
						>
							<span
								class={cn(
									'mt-0.5 grid size-5 shrink-0 place-items-center rounded-sm',
									active ? 'bg-accent/15' : 'bg-input',
								)}
								aria-hidden="true"
							>
								<CatalogIcon name="query" class="size-3" tone="accent" />
							</span>
							<span class="min-w-0 flex-1">
								<span
									class={cn(
										'line-clamp-2 font-mono text-label',
										active ? 'text-accent' : 'text-fg',
									)}
								>
									{previewSql(item.sql)}
								</span>
								<span
									class={cn(
										'flex flex-wrap items-center gap-1.5 text-caption',
										active ? 'text-accent/70' : 'text-fg-muted',
									)}
								>
									<span
										class={item.status === 'success'
											? 'text-success'
											: 'text-danger'}
									>
										{item.status === 'success' ? 'ok' : 'err'}
									</span>
									<span>·</span>
									<span>{formatRelativeTime(item.ranAt)}</span>
									{#if item.durationMs != null}
										<span>·</span>
										<span>{item.durationMs}ms</span>
									{/if}
								</span>
							</span>
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
</aside>

{#if menu}
	<ContextMenu
		x={menu.x}
		y={menu.y}
		items={menuItems}
		ariaLabel="Saved query actions"
		onpick={onMenuPick}
		onclose={() => (menu = null)}
	/>
{/if}
