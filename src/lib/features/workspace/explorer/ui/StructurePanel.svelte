<script lang="ts">
	/**
	 * Object structure — columns, indexes, relations, and engine metadata.
	 * Plugin-owned data; this panel only renders sections.
	 */
	import { commands } from '#core/commands'
	import type { ObjectStructure, StructureSection, TableMeta } from '#core/contracts'
	import {
		Alert,
		Button,
		EmptyHint,
		EmptyState,
		FetchedAgo,
		Icon,
		IconButton,
		Pill,
		SearchField,
		StatusBar,
		ViewLinks,
		type ViewLink,
	} from '#core/ui'
	import {
		cn,
		copyTextToClipboard,
		splitQualifiedName,
	} from '#core/util'
	import { showToast } from '#features/toast'
	import type { IconName } from '#icons'
	import { cmd } from '#lib/app'
	import { onMount } from 'svelte'
	import { SvelteSet } from 'svelte/reactivity'
	import StructureDiff from './StructureDiff.svelte'
	import StructureEditor from './StructureEditor.svelte'

	interface Props {
		structure: ObjectStructure | null
		loading?: boolean
		error?: string | null
		/** Epoch ms of the last engine structure fetch. */
		lastFetchedAt?: number | null
		onretry?: () => void
		onopentable?: () => void
		/** Opens this table's ER diagram; absent when the engine has none. */
		onopenerd?: () => void
		canEdit?: boolean
		readOnly?: boolean
		showForeignKeys?: boolean
		/** Only add columns, rename, and edit indexes (engine capability). */
		editLimited?: boolean
		connectionId?: string | null
		engine?: string
		table?: TableMeta | null
	}

	let {
		structure = null,
		loading = false,
		error = null,
		lastFetchedAt = null,
		onretry,
		onopentable,
		onopenerd,
		canEdit = false,
		readOnly = false,
		showForeignKeys = false,
		editLimited = false,
		connectionId = null,
		engine = '',
		table = null,
	}: Props = $props()

	let editing = $state(false)
	let comparing = $state(false)
	const editable = $derived(
		canEdit && !!table && !!connectionId && structure?.objectKind !== 'view',
	)

	let activeSection = $state<string | null>(null)
	let filter = $state('')
	const expandedCells = new SvelteSet<string>()

	const section = $derived(
		structure?.sections.find((s) => s.id === activeSection) ??
			structure?.sections[0] ??
			null,
	)

	const filteredRows = $derived.by(() => {
		if (!section) return [] as string[][]
		const q = filter.trim().toLowerCase()
		if (!q) return section.rows
		return section.rows.filter((row) =>
			row.some((cell) => cell.toLowerCase().includes(q)),
		)
	})

	const kindIcon = $derived.by((): IconName => {
		const k = structure?.objectKind?.toLowerCase()
		if (k === 'view') return 'view'
		if (k === 'collection') return 'collection'
		if (k === 'table') return 'table'
		return 'structure'
	})

	const title = $derived(splitQualifiedName(structure?.title ?? ''))

	const isColumns = $derived(section?.id === 'columns' || section?.id === 'fields')

	/** Columns that reference another table, read off the foreign-keys section. */
	const fkColumns = $derived.by(() => {
		const fks = structure?.sections.find((s) => s.id === 'foreign_keys')
		const at = fks?.columns.findIndex((c) => c.toLowerCase() === 'column') ?? -1
		return new Set(at < 0 ? [] : fks!.rows.map((r) => r[at] ?? ''))
	})

	/** Engine headers, said the way the table reads. */
	function headerLabel(h: string): string {
		if (!isColumns) return h
		const s = h.toLowerCase()
		if (s === 'pk') return 'Key'
		if (s === 'enum') return 'Enum values'
		return h
	}

	const views = $derived.by(() => {
		const out: ViewLink[] = []
		if (onopentable) out.push({ label: 'Data', icon: 'table', title: 'Open the table data', onclick: onopentable })
		if (onopenerd) out.push({ label: 'ERD', icon: 'schema', title: 'Open the ER diagram', onclick: onopenerd })
		return out
	})

	/** PK / FK / YES chip in a key or flag column. */
	const key =
		'inline-block rounded-[5px] px-1.5 py-0.5 font-mono text-micro leading-none font-semibold tracking-[0.03em]'

	function isEnumHeader(h: string): boolean {
		return h.toLowerCase() === 'enum'
	}

	function selectSection(id: string) {
		activeSection = id
		filter = ''
		expandedCells.clear()
	}

	function cellKey(ri: number, ci: number): string {
		return `${ri}:${ci}`
	}

	function toggleExpand(ri: number, ci: number) {
		const k = cellKey(ri, ci)
		if (expandedCells.has(k)) expandedCells.delete(k)
		else expandedCells.add(k)
	}

	function isNullableHeader(h: string): boolean {
		return h.toLowerCase() === 'nullable'
	}

	function isBoolHeader(h: string): boolean {
		const s = h.toLowerCase()
		return (
			s === 'pk' ||
			s === 'unique' ||
			s === 'primary' ||
			s === 'sparse' ||
			s === 'id'
		)
	}

	function isPkHeader(h: string): boolean {
		const s = h.toLowerCase()
		return s === 'pk' || s === 'primary' || s === 'id'
	}

	function isTypeHeader(h: string): boolean {
		const s = h.toLowerCase()
		return s === 'type' || s === 'sample type' || s.includes('type')
	}

	function isYes(v: string): boolean {
		return v.toLowerCase() === 'yes' || v === 'true' || v === '1'
	}

	function isNo(v: string): boolean {
		return v.toLowerCase() === 'no' || v === 'false' || v === '0'
	}

	function isEmptyish(v: string): boolean {
		return !v || v === '—' || v === '-'
	}

	function isLong(v: string): boolean {
		return v.length > 72 || v.includes('\n')
	}

	function sectionIcon(id: string): IconName {
		switch (id) {
			case 'columns':
			case 'fields':
				return 'table'
			case 'indexes':
				return 'sort'
			case 'foreign_keys':
				return 'external'
			case 'referenced_by':
				return 'link'
			case 'triggers':
				return 'bolt'
			case 'ddl':
				return 'code'
			case 'checks':
				return 'check'
			case 'options':
				return 'settings'
			case 'stats':
				return 'info'
			default:
				return 'structure'
		}
	}

	function notifyCopy(
		res: { ok: boolean; cancelled?: boolean; error?: string },
		okMsg: string,
	) {
		if (res.ok) showToast('success', okMsg)
		else if (!res.cancelled) showToast('danger', res.error || 'Copy failed')
	}

	async function copyWithToast(okMsg: string, text: string) {
		notifyCopy(await copyTextToClipboard(text), okMsg)
	}

	function sectionAsTsv(sec: StructureSection): string {
		const lines = [sec.columns.join('\t')]
		for (const row of sec.rows) {
			lines.push(
				row.map((c) => c.replace(/\t/g, ' ').replace(/\n/g, ' ')).join('\t'),
			)
		}
		return lines.join('\n')
	}

	function onSectionKeydown(e: KeyboardEvent) {
		if (!structure?.sections.length) return
		const ids = structure.sections.map((s) => s.id)
		const i = Math.max(0, ids.indexOf(activeSection ?? ids[0]!))
		if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
			e.preventDefault()
			selectSection(ids[Math.min(ids.length - 1, i + 1)]!)
		} else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
			e.preventDefault()
			selectSection(ids[Math.max(0, i - 1)]!)
		} else if (e.key === 'Home') {
			e.preventDefault()
			selectSection(ids[0]!)
		} else if (e.key === 'End') {
			e.preventDefault()
			selectSection(ids[ids.length - 1]!)
		}
	}

	// ⌘F or / outside a text field focuses the filter.
	onMount(() =>
		commands.register({
			...cmd.structureFind,
			run: () => document.getElementById('structure-filter')?.focus(),
		}),
	)
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col bg-app">
	{#if comparing && table && connectionId}
		<StructureDiff
			{connectionId}
			{engine}
			{table}
			onclose={() => (comparing = false)}
		/>
	{/if}
	{#if editing && table && connectionId}
		<StructureEditor
			{connectionId}
			{table}
			{readOnly}
			{showForeignKeys}
			limited={editLimited}
			ondone={() => (editing = false)}
		/>
	{:else if loading && !structure}
		<div class="flex min-h-0 flex-1 flex-col" aria-busy="true">
			<div
				class="flex h-12 shrink-0 items-center gap-2.5 border-b border-line bg-surface pr-3 pl-4"
			>
				<div class="size-6 animate-pulse rounded-md bg-field"></div>
				<div class="h-3 w-36 animate-pulse rounded-sm bg-field"></div>
				<div
					class="ml-auto h-control-sm w-52 animate-pulse rounded-md bg-field"
				></div>
			</div>
			<div
				class="flex h-10.5 shrink-0 items-center gap-0.5 overflow-hidden border-b border-line bg-surface px-2.5"
			>
				{#each [1, 2, 3, 4] as i (i)}
					<div
						class="flex items-center gap-1.5 px-2.5"
						style="animation-delay: {i * 40}ms"
					>
						<div class="h-3 w-16 animate-pulse rounded-sm bg-field"></div>
					</div>
				{/each}
			</div>
			<div class="min-h-0 flex-1">
				<div class="flex h-8.5 items-center gap-4 border-b border-line bg-surface px-4">
					<div class="h-3 w-8 animate-pulse rounded-sm bg-field"></div>
					<div class="h-3 w-24 animate-pulse rounded-sm bg-field"></div>
					<div class="h-3 w-20 animate-pulse rounded-sm bg-field/70"></div>
					<div class="h-3 flex-1 animate-pulse rounded-sm bg-field/50"></div>
				</div>
				{#each [1, 2, 3, 4, 5, 6, 7, 8] as r (r)}
					<div
						class="flex h-10 items-center gap-4 border-b border-line px-4"
					>
						<div class="h-3 w-6 animate-pulse rounded-sm bg-field/60"></div>
						<div class="h-3 w-28 animate-pulse rounded-sm bg-field"></div>
						<div class="h-3 w-20 animate-pulse rounded-sm bg-field/70"></div>
						<div class="h-3 w-12 animate-pulse rounded-sm bg-field/50"></div>
					</div>
				{/each}
			</div>
			<StatusBar ariaLabel="Structure status">
				{#snippet left()}
					<span class="text-fg-muted">Loading…</span>
				{/snippet}
			</StatusBar>
		</div>
	{:else if error && !structure}
		<div class="flex flex-1 items-center justify-center p-8">
			<EmptyState title="Could not load structure" description={error}>
				{#snippet actions()}
					{#if onretry}
						<Button variant="secondary" size="sm" onclick={onretry}>
							Retry
						</Button>
					{/if}
				{/snippet}
			</EmptyState>
		</div>
	{:else if structure}
		<header class="flex h-12 shrink-0 items-center gap-2.5 border-b border-line bg-surface pr-3 pl-4">
			<div class="flex min-w-0 items-center gap-2">
				<span class="flex shrink-0 text-accent-text [&_svg]:size-4" aria-hidden="true">
					<Icon name={kindIcon} />
				</span>
				<h1 class="m-0 min-w-0 truncate font-mono text-[0.84375rem] font-normal tracking-[-0.01em]" title={structure.title}>
					{#if title.schema}<span class="text-fg-muted">{title.schema}</span>{/if}<span
						class="text-fg">{title.name}</span
					>
				</h1>
				{#if structure.objectKind}
					<Pill uppercase>{structure.objectKind}</Pill>
				{/if}
			</div>

			<span class="min-w-0 flex-1"></span>

			<div class="w-55 min-w-0 shrink">
				<SearchField
					bind:value={filter}
					id="structure-filter"
					placeholder={section ? `Filter ${section.title.toLowerCase()}…` : 'Filter…'}
					label={section ? `Filter ${section.title.toLowerCase()}` : 'Filter structure'}
				/>
			</div>
			{#if editable}
				<Button variant="secondary" onclick={() => (comparing = true)}>
					<Icon name="sort" />Compare
				</Button>
				<Button variant="primary" onclick={() => (editing = true)}>
					<Icon name="pencil" />Edit
				</Button>
			{/if}
		</header>

		{#if error}
			<div class="shrink-0 px-2.5 pt-2">
				<Alert tone="danger">
					<div class="flex items-center justify-between gap-2">
						<span class="min-w-0 truncate">{error}</span>
						{#if onretry}
							<Button variant="ghost" size="sm" onclick={onretry}>Retry</Button>
						{/if}
					</div>
				</Alert>
			</div>
		{/if}

		{#if structure.sections.length > 0}
			<div
				class="scrollbar-none flex h-10.5 shrink-0 items-stretch gap-0.5 overflow-x-auto border-b border-line bg-surface px-2.5"
				role="tablist"
				aria-label="Structure sections"
				aria-orientation="horizontal"
			>
				{#each structure.sections as sec (sec.id)}
					{@const active = section?.id === sec.id}
					<button
						type="button"
						role="tab"
						aria-selected={active}
						tabindex={active ? 0 : -1}
						class={cn(
							'relative inline-flex shrink-0 items-center gap-[7px] border-none bg-transparent px-[11px] text-[0.78125rem] font-medium transition-colors duration-150',
							active ? 'text-fg' : 'text-fg-muted hover:text-fg',
						)}
						onclick={() => selectSection(sec.id)}
						onkeydown={onSectionKeydown}
					>
						<span
							class={cn('flex [&_svg]:size-3.5', active ? 'text-accent-text' : 'text-fg-muted')}
							aria-hidden="true"
						>
							<Icon name={sectionIcon(sec.id)} />
						</span>
						{sec.title}
						<span
							class={cn(
								'rounded-[5px] px-1.5 py-px font-mono text-[0.65625rem] tabular-nums',
								active ? 'bg-accent-soft text-accent-text' : 'bg-hover text-fg-muted',
							)}
						>
							{sec.rows.length}
						</span>
						{#if active}
							<span class="absolute inset-x-2.5 -bottom-px h-0.5 rounded-full bg-accent" aria-hidden="true"></span>
						{/if}
					</button>
				{/each}
			</div>
		{/if}

		<div class="flex min-h-0 min-w-0 flex-1 flex-col">
			{#if section}
				{#if section.rows.length === 0}
					<EmptyHint
						fill
						icon={sectionIcon(section.id)}
						title="No {section.title.toLowerCase()}"
						description={section.emptyMessage || `No ${section.title.toLowerCase()} on this object.`}
					/>
				{:else if filteredRows.length === 0}
					<EmptyHint
						fill
						icon="search"
						title="No matches"
						description="Nothing in {section.title.toLowerCase()} matches “{filter.trim()}”"
					>
						<Button variant="ghost" size="sm" onclick={() => (filter = '')}>Clear filter</Button>
					</EmptyHint>
				{:else}
					<div class="min-h-0 flex-1 overflow-auto">
						<table class="w-full border-collapse text-left">
							<thead class="sticky top-0 z-10 bg-surface">
								<tr class="h-8.5 shadow-[inset_0_-1px_0_var(--line)]">
									<th class="w-11 pr-2 pl-4 text-right font-mono text-caption font-normal text-fg-muted" scope="col">#</th>
									{#each section.columns as col (col)}
										<th class="px-2 text-[0.71875rem] font-medium whitespace-nowrap text-fg-muted" scope="col">
											{headerLabel(col)}
										</th>
									{/each}
									<th class="w-10 pr-3 pl-1" scope="col"><span class="sr-only">Copy</span></th>
								</tr>
							</thead>
							<tbody>
								{#each filteredRows as row, ri (ri)}
									<tr class="group/row border-b border-line text-body transition-colors duration-150 hover:bg-surface">
										<td class="h-10 pr-2 pl-4 text-right font-mono text-caption text-fg-muted tabular-nums">
											{ri + 1}
										</td>
										{#each row as cell, ci (ci)}
											{@const header = section.columns[ci] ?? ''}
											{@const expanded = expandedCells.has(cellKey(ri, ci))}
											{@const fk = isColumns && ci > 0 && isPkHeader(header) && fkColumns.has(row[0] ?? '')}
											<td class="max-w-md px-2 py-1.5 align-middle">
												{#if isPkHeader(header) && (isYes(cell) || fk)}
													<span class={cn(key, isYes(cell) ? 'bg-accent-soft text-accent-text' : 'bg-info-soft text-info')}>
														{isYes(cell) ? 'PK' : 'FK'}
													</span>
												{:else if isEmptyish(cell)}
													<span class="text-fg-muted" aria-label="None">—</span>
												{:else if isNullableHeader(header) && (isYes(cell) || isNo(cell))}
													{#if isYes(cell)}
														<span title="Nullable">✓</span><span class="sr-only">Nullable</span>
													{:else}
														<span title="Not nullable">—</span>
														<span class="sr-only">Not nullable</span>
													{/if}
												{:else if isBoolHeader(header) && (isYes(cell) || isNo(cell))}
													{#if isYes(cell)}
														<span class={cn(key, 'bg-accent-soft text-accent-text')}>YES</span>
													{:else}
														<span class="text-fg-muted">—</span><span class="sr-only">No</span>
													{/if}
												{:else if isColumns && isEnumHeader(header)}
													<span class="flex max-w-72 flex-wrap gap-1">
														{#each cell.split(/,\s*/) as value, vi (vi)}
															<span class="rounded-[5px] bg-violet-soft px-1.5 py-px font-mono text-caption leading-normal text-violet">
																{value}
															</span>
														{/each}
													</span>
												{:else if isTypeHeader(header)}
													<code
														class="inline-block rounded-md bg-hover px-[7px] py-0.5 font-mono text-[0.71875rem] leading-normal whitespace-nowrap text-fg-secondary shadow-[inset_0_0_0_1px_var(--line-2)]"
													>
														{cell}
													</code>
												{:else if isLong(cell)}
													<div class="flex min-w-0 items-start gap-1">
														<pre
															class={cn(
																'm-0 min-w-0 flex-1 overflow-x-auto rounded-md bg-field px-2 py-1.5 font-mono text-label leading-relaxed break-all whitespace-pre-wrap text-fg-secondary shadow-[inset_0_0_0_1px_var(--line)]',
																!expanded && 'line-clamp-3',
															)}>{cell}</pre>
														<IconButton
															label={expanded ? 'Collapse' : 'Expand'}
															size="sm"
															variant="quiet"
															onclick={() => toggleExpand(ri, ci)}
														>
															<Icon
																name="chevron-down"
																class={cn('transition-transform duration-150', expanded && 'rotate-180')}
															/>
														</IconButton>
													</div>
												{:else if ci === 0}
													<span class="font-mono font-medium whitespace-nowrap text-fg" title={cell}>{cell}</span>
												{:else}
													<span class="block truncate font-mono text-label text-fg-secondary" title={cell}>{cell}</span>
												{/if}
											</td>
										{/each}
										<td class="pr-3 pl-1 align-middle">
											<div class="opacity-0 transition-opacity group-hover/row:opacity-100 focus-within:opacity-100">
												<IconButton
													label="Copy row"
													size="sm"
													variant="quiet"
													onclick={() => void copyWithToast('Row copied', row.join('\t'))}
												>
													<Icon name="copy" />
												</IconButton>
											</div>
										</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			{:else}
				<div class="flex flex-1 items-center justify-center p-8">
					<p class="m-0 text-caption text-fg-muted">No structure sections.</p>
				</div>
			{/if}
		</div>

		<StatusBar class={views.length ? 'pl-1.5!' : ''} ariaLabel="Structure status">
			{#snippet left()}
				<ViewLinks links={views} />
			{/snippet}
			{#snippet right()}
				{#if lastFetchedAt != null}
					<FetchedAgo at={lastFetchedAt} class="mr-1" />
				{/if}
				<IconButton
					label="Copy this section as TSV"
					size="sm"
					variant="quiet"
					disabled={!section || section.rows.length === 0}
					onclick={() => {
						if (!section) return
						void copyWithToast('Section copied', sectionAsTsv(section))
					}}
				>
					<Icon name="copy" />
				</IconButton>
				{#if onretry}
					<IconButton
						label={loading ? 'Refreshing structure' : 'Refresh structure'}
						size="sm"
						variant="quiet"
						disabled={loading}
						aria-busy={loading}
						onclick={onretry}
						class={loading ? 'opacity-45' : ''}
					>
						<Icon name="refresh" class={cn(loading && 'animate-spin')} />
					</IconButton>
				{/if}
			{/snippet}
		</StatusBar>
	{:else}
		<EmptyHint
			fill
			icon="structure"
			title="Object structure"
			description="Right-click a table or collection and choose View structure."
		/>
	{/if}
</div>
