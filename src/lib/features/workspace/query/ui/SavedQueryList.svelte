<script lang="ts">
	import { ContextMenu, Icon, type ContextMenuItem } from '$core/ui'
	import { cn, formatRelativeTime } from '$core/util'
	import { CatalogIcon } from '$features/workspace/catalog'
	import type { SavedQueryListItem } from '../saved-types'

	export type SavedQueryMenuAction =
		| 'open'
		| 'favorite'
		| 'duplicate'
		| 'rename'
		| 'delete'

	interface Props {
		items: SavedQueryListItem[]
		query?: string
		selectedId: string | null
		onselect: (id: string) => void
		onaction?: (action: SavedQueryMenuAction, id: string) => void
	}

	let { items, query = '', selectedId, onselect, onaction }: Props = $props()

	let menu = $state<{ id: string; x: number; y: number } | null>(null)

	const menuItems = $derived.by((): ContextMenuItem[] => {
		const current = items.find((s) => s.id === menu?.id)
		return [
			{ id: 'open', label: 'Open' },
			{
				id: 'favorite',
				label: current?.favorite
					? 'Remove from favourites'
					: 'Add to favourites',
			},
			{ id: 'duplicate', label: 'Duplicate' },
			{ id: 'rename', label: 'Rename' },
			{ id: 'delete', label: 'Delete', danger: true, separatorBefore: true },
		]
	})

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
		onaction?.(action as SavedQueryMenuAction, id)
	}
</script>

{#if items.length === 0}
	<p class="m-2 text-caption leading-relaxed text-fg-muted">
		{#if query.trim()}
			No matches for “{query.trim()}”.
		{:else}
			No saved queries yet. Save a query from the editor.
		{/if}
	</p>
{:else}
	<ul class="m-0 list-none p-0">
	{#each items as item (item.id)}
		{@const active = selectedId === item.id}
		<li
			class={cn(
				'group flex items-start rounded-md',
				active ? 'bg-accent-muted text-accent' : 'text-fg hover:bg-hover',
			)}
		>
			<button
				type="button"
				class="flex min-w-0 flex-1 items-start gap-1.5 border-none bg-transparent px-2 py-1.5 text-left"
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
					<span class="block truncate text-label font-medium">{item.name}</span>
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
			<button
				type="button"
				class={cn(
					'mt-1.5 mr-1 grid size-6 shrink-0 place-items-center rounded-sm border-none bg-transparent transition-colors',
					item.favorite
						? 'text-accent'
						: 'text-fg-muted opacity-0 hover:text-fg group-hover:opacity-100',
					'focus-visible:opacity-100',
				)}
				aria-label={item.favorite
					? 'Remove from favourites'
					: 'Add to favourites'}
				aria-pressed={item.favorite}
				onclick={() => onaction?.('favorite', item.id)}
			>
				<Icon name={item.favorite ? 'star-filled' : 'star'} class="size-3.5" />
			</button>
		</li>
	{/each}
	</ul>
{/if}

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
