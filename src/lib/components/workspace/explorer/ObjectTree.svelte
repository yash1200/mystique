<script lang="ts">
	import type { DatabaseMeta, TableMeta } from '$lib/types/workspace'
	import { cn } from '$lib/utils/cn'
	import CatalogIcon from '../CatalogIcon.svelte'

	interface Props {
		objectLabel: string
		groupLabel: string
		isTree: boolean
		selectedDatabase: string
		currentDb: DatabaseMeta | null
		totalCount: number
		query: string
		activeTableId: string | null
		groups: { key: string; items: TableMeta[] }[]
		flatItems: TableMeta[]
		isExpanded: (key: string) => boolean
		onToggleGroup: (key: string) => void
		onopen: (table: TableMeta) => void
		onmenu: (table: TableMeta, e: MouseEvent) => void
	}

	let {
		objectLabel,
		groupLabel,
		isTree,
		selectedDatabase,
		currentDb,
		totalCount,
		query,
		activeTableId,
		groups,
		flatItems,
		isExpanded,
		onToggleGroup,
		onopen,
		onmenu,
	}: Props = $props()
</script>

{#snippet objectRow(t: TableMeta, nested: boolean)}
	{@const active = activeTableId === t.id}
	<li role="treeitem" aria-selected={active}>
		<button
			type="button"
			class={cn(
				'flex h-control-sm w-full items-center gap-1.5 rounded-md border-none px-1.5 text-left transition-colors',
				nested && 'pl-1',
				active
					? 'bg-accent-muted text-accent'
					: 'bg-transparent text-fg hover:bg-hover',
			)}
			title={t.schema ? `${t.schema}.${t.name}` : t.name}
			ondblclick={() => onopen(t)}
			oncontextmenu={(e) => {
				e.preventDefault()
				e.stopPropagation()
				onmenu(t, e)
			}}
		>
			<CatalogIcon name={t.kind} {active} />
			<span class="min-w-0 flex-1 truncate font-mono text-label">
				{t.name}
			</span>
			{#if t.kind === 'view'}
				<span
					class={cn(
						'shrink-0 font-mono text-micro tracking-wide uppercase',
						active ? 'text-accent' : 'text-fg-muted',
					)}
				>
					view
				</span>
			{/if}
		</button>
	</li>
{/snippet}

<div
	class="flex-1 overflow-auto px-1.5 py-1.5"
	role="tree"
	aria-label="{objectLabel} in {selectedDatabase}"
>
	{#if !currentDb}
		<p class="m-2 text-caption leading-relaxed text-fg-muted">
			No database selected.
		</p>
	{:else if totalCount === 0}
		<p class="m-2 text-caption leading-relaxed text-fg-muted">
			No {objectLabel} match
			{#if query.trim()}
				for “{query.trim()}”
			{:else}
				in <span class="font-mono text-fg-secondary">{currentDb.name}</span>
			{/if}.
		</p>
	{:else if isTree}
		{#each groups as group (group.key)}
			{@const expanded = isExpanded(group.key)}
			<section class="mb-0.5" aria-label="{groupLabel} {group.key}">
				<button
					type="button"
					class={cn(
						'sticky top-0 z-10 flex h-control-sm w-full items-center gap-1 rounded-md border-none bg-panel px-1.5 text-left transition-colors',
						'text-fg-secondary hover:bg-hover hover:text-fg',
					)}
					aria-expanded={expanded}
					title="{groupLabel} {group.key}"
					onclick={() => onToggleGroup(group.key)}
				>
					<CatalogIcon
						name="chevron"
						class={cn(
							'size-3 text-fg-muted transition-transform duration-150',
							expanded ? 'rotate-90' : 'rotate-0',
						)}
					/>
					<CatalogIcon name="schema" />
					<span class="min-w-0 flex-1 truncate font-mono text-label">
						{group.key}
					</span>
					<span
						class="shrink-0 font-mono text-micro text-fg-muted tabular-nums"
					>
						{group.items.length}
					</span>
				</button>

				{#if expanded}
					<ul class="m-0 list-none py-0.5 pl-5" role="group">
						{#each group.items as t (t.id)}
							{@render objectRow(t, true)}
						{/each}
					</ul>
				{/if}
			</section>
		{/each}
	{:else}
		<ul class="m-0 list-none p-0" role="group">
			{#each flatItems as t (t.id)}
				{@render objectRow(t, false)}
			{/each}
		</ul>
	{/if}
</div>
