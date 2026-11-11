<script lang="ts">
	import type { DatabaseMeta, ObjectKindSpec, TableMeta } from '#core/contracts'
	import {
		EmptyHint,
		Icon,
	} from '#core/ui'
	import { cn } from '#core/util'
	import {
		CatalogIcon,
		iconToneForName,
		writeTableDrag,
	} from '#features/workspace/catalog'
	import { appPrefs } from '#lib/app'
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
		/** Plugin spec for a catalog kind (heading, primary or grouped). */
		kindOf: (kind: string) => ObjectKindSpec
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
		kindOf,
		busy = false,
		onToggleGroup,
		onopen,
		onmenu,
	}: Props = $props()


	function wellClass(kind: TableMeta['kind'], active: boolean): string {
		if (active) return 'bg-accent-soft'
		if (kind === 'view') return 'bg-info/15'
		if (kind === 'table' || kind === 'collection') return 'bg-accent-soft'
		return 'bg-field'
	}

	type KindSection = { spec: ObjectKindSpec; items: TableMeta[] }

	/**
	 * Tables, views and collections stay in the list; every other kind
	 * (functions, sequences, types…) goes under its own collapsed heading.
	 * `items` is already in the plugin's kind order.
	 */
	function splitByKind(items: TableMeta[]): {
		primary: TableMeta[]
		sections: KindSection[]
	} {
		const primary: TableMeta[] = []
		const sections: KindSection[] = []
		for (const t of items) {
			const spec = kindOf(t.kind)
			if (spec.primary) {
				primary.push(t)
				continue
			}
			const last = sections.at(-1)
			if (last && last.spec.id === spec.id) last.items.push(t)
			else sections.push({ spec, items: [t] })
		}
		return { primary, sections }
	}

	/** Open kind sections, keyed `group::kind`. Closed by default. */
	let openSections = $state<Set<string>>(new Set())

	function sectionOpen(key: string): boolean {
		return query.trim().length > 0 || openSections.has(key)
	}

	function toggleSection(key: string) {
		const next = new Set(openSections)
		if (next.has(key)) next.delete(key)
		else next.add(key)
		openSections = next
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
			<mark class="rounded-sm bg-accent-soft p-0 text-fg">{part.text}</mark>
		{:else}
			{part.text}
		{/if}
	{/each}
{/snippet}

{#snippet objectRow(t: TableMeta)}
	{@const active = activeTableId === t.id}
	<li
		role="treeitem"
		aria-selected={active}
		aria-current={active ? 'true' : undefined}
	>
		<button
			type="button"
			class={cn(
				'relative flex h-control-sm w-full items-center gap-1.5 rounded-md border-none px-1.5 text-left transition-colors duration-150',
				'focus-visible:outline-accent focus-visible:outline-2 focus-visible:outline-offset-1',
				active ? 'bg-accent-soft' : 'bg-transparent hover:bg-hover',
			)}
			aria-label="{t.name}, {t.kind}"
			title={objectTitle(t)}
			draggable="true"
			ondragstart={(e) => {
				if (!e.dataTransfer) return
				writeTableDrag(e.dataTransfer, {
					database: t.database,
					schema: t.schema,
					name: t.name,
				})
			}}
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
		</button>
	</li>
{/snippet}

{#snippet objectList(items: TableMeta[], groupKey: string)}
	{@const split = splitByKind(items)}
	{#each split.primary as t (t.id)}
		{@render objectRow(t)}
	{/each}
	{#each split.sections as section (section.spec.id)}
		{@const key = `${groupKey}::${section.spec.id}`}
		{@const open = sectionOpen(key)}
		<li role="none">
			<button
				type="button"
				class={cn(
					'flex h-control-sm w-full items-center gap-1.5 rounded-md border-none bg-transparent px-1.5 text-left transition-colors duration-150',
					'text-fg-muted hover:bg-hover hover:text-fg',
					'focus-visible:outline-accent focus-visible:outline-2 focus-visible:outline-offset-1',
				)}
				aria-expanded={open}
				onclick={() => toggleSection(key)}
			>
				<CatalogIcon
					name="chevron"
					class={cn(
						'size-3 transition-transform duration-150',
						open ? 'rotate-90' : 'rotate-0',
					)}
				/>
				<span class="min-w-0 flex-1 truncate text-caption">
					{section.spec.plural}
				</span>
				<span
					class="shrink-0 rounded-sm bg-field px-1 font-mono text-micro text-fg-muted tabular-nums"
				>
					{section.items.length}
				</span>
			</button>
			{#if open}
				<ul
					class="m-0 ml-2.5 list-none border-l border-line py-0.5 pl-1.5"
					role="group"
					aria-label={section.spec.plural}
				>
					{#each section.items as t (t.id)}
						{@render objectRow(t)}
					{/each}
				</ul>
			{/if}
		</li>
	{/each}
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
		<EmptyHint
			title="No database selected"
			description="Choose a database above to browse {objectLabel}."
		>
			{#snippet glyph()}<CatalogIcon name="database" class="size-4" />{/snippet}
		</EmptyHint>
	{:else if busy && totalCount === 0 && groups.length === 0 && !query.trim()}
		<EmptyHint title="Loading {objectLabel}…">
			{#snippet glyph()}<Icon name="refresh" class="animate-spin" />{/snippet}
		</EmptyHint>
	{:else if totalCount === 0 && groups.length === 0}
		{#if query.trim()}
			<EmptyHint icon="search" title="No matches" description="Nothing matches “{query.trim()}”." />
		{:else}
			<EmptyHint
				icon="table"
				title="No {objectLabel}"
				description="{currentDb.name} has no {objectLabel} yet."
			/>
		{/if}
	{:else if isTree}
		{#each groups as group (group.key)}
			{@const expanded = isExpanded(group.key)}
			<section class="mb-0.5" aria-label="{groupLabel} {group.key}">
				<button
					type="button"
					class={cn(
						'sticky top-0 z-10 flex h-control-sm w-full items-center gap-1.5 rounded-md border-none bg-surface/95 px-1.5 text-left transition-colors duration-150',
						'text-fg-secondary hover:bg-hover hover:text-fg',
						'focus-visible:outline-accent focus-visible:outline-2 focus-visible:outline-offset-1',
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
						class="shrink-0 rounded-sm bg-field px-1 font-mono text-micro text-fg-muted tabular-nums"
					>
						{group.items.length}
					</span>
				</button>

				{#if expanded}
					<ul
						class="m-0 ml-2.5 list-none border-l border-line py-0.5 pl-1.5"
						role="group"
					>
						{@render objectList(group.items, group.key)}
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
			{@render objectList(flatItems, selectedDatabase)}
		</ul>
	{/if}
</div>
