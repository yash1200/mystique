<script lang="ts">
	import type { DatabaseMeta, TableMeta } from '$lib/types/workspace'
	import { cn } from '$lib/utils/cn'
	import { iconToneForName } from '../catalogIcon'
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

	function formatCount(n: number): string {
		if (!Number.isFinite(n) || n <= 0) return ''
		if (n < 1000) return String(n)
		if (n < 10_000) return `${(n / 1000).toFixed(1).replace(/\.0$/, '')}k`
		if (n < 1_000_000) return `${Math.round(n / 1000)}k`
		return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, '')}m`
	}

	function wellClass(kind: TableMeta['kind'], active: boolean): string {
		if (active) return 'bg-accent-muted'
		if (kind === 'view') return 'bg-info/15'
		if (kind === 'table' || kind === 'collection') return 'bg-success/15'
		return 'bg-input'
	}
</script>

{#snippet objectRow(t: TableMeta)}
	{@const active = activeTableId === t.id}
	{@const count = formatCount(t.rowCount)}
	<li
		role="treeitem"
		aria-selected={active}
		aria-current={active ? 'true' : undefined}
	>
		<button
			type="button"
			class={cn(
				'flex h-control-sm w-full items-center gap-1.5 rounded-md border-none px-1.5 text-left transition-colors duration-150',
				'focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1',
				active ? 'bg-accent-muted' : 'bg-transparent hover:bg-hover',
			)}
			aria-label="{t.name}, {t.kind}"
			title={t.schema ? `${t.schema}.${t.name}` : t.name}
			ondblclick={() => onopen(t)}
			oncontextmenu={(e) => {
				e.preventDefault()
				e.stopPropagation()
				onmenu(t, e)
			}}
		>
			<span
				class={cn(
					'grid size-5 shrink-0 place-items-center rounded-sm transition-colors duration-150',
					wellClass(t.kind, active),
				)}
				aria-hidden="true"
			>
				<CatalogIcon
					name={t.kind}
					class="size-3"
					tone={iconToneForName(t.kind, active)}
				/>
			</span>
			<span
				class={cn(
					'min-w-0 flex-1 truncate font-mono text-label',
					active ? 'font-medium text-fg' : 'text-fg',
				)}
			>
				{t.name}
			</span>
			{#if count}
				<span
					class={cn(
						'shrink-0 font-mono text-micro tabular-nums',
						active ? 'text-accent' : 'text-fg-muted',
					)}
				>
					{count}
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
	{:else if totalCount === 0 && groups.length === 0}
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
			<section class="mb-1" aria-label="{groupLabel} {group.key}">
				<button
					type="button"
					class={cn(
						'sticky top-0 z-10 flex h-control-sm w-full items-center gap-1.5 rounded-md border-none bg-panel px-1.5 text-left transition-colors duration-150',
						'text-fg-secondary hover:bg-hover hover:text-fg',
						'focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1',
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
					<CatalogIcon name="schema" class="size-3.5" />
					<span class="min-w-0 flex-1 truncate font-mono text-label">
						{group.key}
					</span>
					<span
						class="shrink-0 rounded-sm bg-input px-1 font-mono text-micro text-fg-muted tabular-nums"
					>
						{group.items.length}
					</span>
				</button>

				{#if expanded}
					<ul
						class="m-0 ml-2.5 list-none border-l border-line py-0.5 pl-1.5"
						role="group"
					>
						{#each group.items as t (t.id)}
							{@render objectRow(t)}
						{/each}
						{#if group.items.length === 0}
							<li class="px-1.5 py-1 text-caption text-fg-muted">
								No {objectLabel}
							</li>
						{/if}
					</ul>
				{/if}
			</section>
		{/each}
	{:else}
		<ul class="m-0 flex list-none flex-col gap-px p-0" role="group">
			{#each flatItems as t (t.id)}
				{@render objectRow(t)}
			{/each}
		</ul>
	{/if}
</div>
