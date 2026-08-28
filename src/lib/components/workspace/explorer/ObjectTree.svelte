<script lang="ts">
	import { Icon } from '$lib/components/core'
	import { appPrefs } from '$lib/stores/app-prefs.svelte'
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
		busy?: boolean
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
		busy = false,
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

	function matchParts(
		text: string,
		q: string,
	): { text: string; hit: boolean }[] {
		const needle = q.trim()
		if (!needle) return [{ text, hit: false }]
		const i = text.toLowerCase().indexOf(needle.toLowerCase())
		if (i < 0) return [{ text, hit: false }]
		return [
			{ text: text.slice(0, i), hit: false },
			{ text: text.slice(i, i + needle.length), hit: true },
			{ text: text.slice(i + needle.length), hit: false },
		].filter((part) => part.text.length > 0)
	}

	function openFromClick(e: MouseEvent, table: TableMeta) {
		if (e.detail > 1) return
		// Keyboard activation reports detail 0; always open so Enter/Space work
		// when the mouse gesture is double-click.
		if (appPrefs.openTableOnSingleClick || e.detail === 0) onopen(table)
	}

	function openFromDblClick(table: TableMeta) {
		if (!appPrefs.openTableOnSingleClick) onopen(table)
	}

	function objectTitle(table: TableMeta): string {
		const name = table.schema ? `${table.schema}.${table.name}` : table.name
		const hint = appPrefs.openTableOnSingleClick
			? 'Click to open'
			: 'Double-click to open'
		return `${name} · ${hint}`
	}
</script>

{#snippet highlighted(text: string)}
	{#each matchParts(text, query) as part, i (i)}
		{#if part.hit}
			<mark class="rounded-sm bg-accent-muted p-0 text-fg">{part.text}</mark>
		{:else}
			{part.text}
		{/if}
	{/each}
{/snippet}

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
				'relative flex h-control-sm w-full items-center gap-1.5 rounded-md border-none px-1.5 text-left transition-colors duration-150',
				'focus-visible:outline-focus focus-visible:outline-2 focus-visible:outline-offset-1',
				active ? 'bg-accent-muted' : 'bg-transparent hover:bg-hover',
			)}
			aria-label="{t.name}, {t.kind}"
			title={objectTitle(t)}
			onclick={(e) => openFromClick(e, t)}
			ondblclick={() => openFromDblClick(t)}
			oncontextmenu={(e) => {
				e.preventDefault()
				e.stopPropagation()
				onmenu(t, e)
			}}
		>
			<span
				class={cn(
					'absolute inset-y-1 left-0 w-0.5 rounded-full transition-colors duration-150',
					active ? 'bg-accent' : 'bg-transparent',
				)}
				aria-hidden="true"
			></span>
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
				{@render highlighted(t.name)}
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
	class={cn(
		'flex-1 overflow-auto px-1.5 py-1 transition-opacity duration-150',
		busy && 'opacity-60',
	)}
	role="tree"
	aria-label="{objectLabel} in {selectedDatabase}"
	aria-busy={busy || undefined}
>
	{#if !currentDb}
		<div
			class="flex flex-col items-center justify-center px-3 py-8 text-center"
		>
			<span
				class="mb-2 grid size-8 place-items-center rounded-lg bg-input"
				aria-hidden="true"
			>
				<CatalogIcon name="database" class="size-4" />
			</span>
			<p class="m-0 text-label font-medium text-fg">No database selected</p>
			<p class="mt-0.5 text-caption leading-relaxed text-fg-muted">
				Choose a database above to browse {objectLabel}.
			</p>
		</div>
	{:else if totalCount === 0 && groups.length === 0}
		<div
			class="flex flex-col items-center justify-center px-3 py-8 text-center"
		>
			<span
				class="mb-2 grid size-8 place-items-center rounded-lg bg-input"
				aria-hidden="true"
			>
				<Icon
					name={query.trim() ? 'search' : 'table'}
					class="size-4 text-fg-muted"
				/>
			</span>
			{#if query.trim()}
				<p class="m-0 text-label font-medium text-fg">No matches</p>
				<p class="mt-0.5 text-caption leading-relaxed text-fg-muted">
					Nothing matches “{query.trim()}”.
				</p>
			{:else}
				<p class="m-0 text-label font-medium text-fg">
					No {objectLabel}
				</p>
				<p class="mt-0.5 text-caption leading-relaxed text-fg-muted">
					<span class="font-mono text-fg-secondary">{currentDb.name}</span>
					has no {objectLabel} yet.
				</p>
			{/if}
		</div>
	{:else if isTree}
		{#each groups as group (group.key)}
			{@const expanded = isExpanded(group.key)}
			<section class="mb-0.5" aria-label="{groupLabel} {group.key}">
				<button
					type="button"
					class={cn(
						'sticky top-0 z-10 flex h-control-sm w-full items-center gap-1.5 rounded-md border-none bg-panel/95 px-1.5 text-left transition-colors duration-150',
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
						{@render highlighted(group.key)}
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
