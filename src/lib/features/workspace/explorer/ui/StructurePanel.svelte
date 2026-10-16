<script lang="ts">
	/**
	 * Object structure — columns, indexes, relations, and engine metadata.
	 * Plugin-owned data; this panel only renders sections.
	 */
	import type { ObjectStructure, StructureSection, TableMeta } from '#core/contracts'
	import {
		Alert,
		Button,
		EmptyState,
		Icon,
		IconButton,
		Pill,
		StatusBar,
	} from '#core/ui'
	import {
		cn,
		copyTextToClipboard,
		fetchedLabel,
		randomLoadingPun,
	} from '#core/util'
	import { showToast } from '#features/toast'
	import type { CatalogIconName } from '#features/workspace/catalog'
	import { CatalogIcon, iconToneForName } from '#features/workspace/catalog'
	import type { IconName } from '#icons'
	import { SvelteSet } from 'svelte/reactivity'
	import ExplorerSearch from './ExplorerSearch.svelte'
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
		canEdit?: boolean
		readOnly?: boolean
		showForeignKeys?: boolean
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
		canEdit = false,
		readOnly = false,
		showForeignKeys = false,
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
	const loadingPun = randomLoadingPun()
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

	const kindIcon = $derived.by((): CatalogIconName => {
		const k = structure?.objectKind?.toLowerCase()
		if (k === 'view') return 'view'
		if (k === 'collection') return 'collection'
		if (k === 'table') return 'table'
		return 'structure'
	})

	const openTableLabel = $derived.by(() => {
		const k = structure?.objectKind?.toLowerCase()
		if (k === 'collection') return 'Collection'
		if (k === 'view') return 'View'
		return 'Table'
	})

	const openTableIcon = $derived.by((): IconName => {
		const k = structure?.objectKind?.toLowerCase()
		if (k === 'view') return 'view'
		if (k === 'collection') return 'collection'
		return 'table'
	})

	let now = $state(Date.now())

	$effect(() => {
		const id = window.setInterval(() => {
			now = Date.now()
		}, 15_000)
		return () => window.clearInterval(id)
	})

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
			case 'referenced_by':
				return 'external'
			case 'triggers':
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
		else if (res.cancelled) showToast('info', 'Copy cancelled')
		else showToast('danger', res.error || 'Copy failed')
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

	function onFindKeydown(e: KeyboardEvent) {
		const find =
			e.key === '/' || ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'f')
		if (!find) return
		const t = e.target
		if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement) {
			return
		}
		e.preventDefault()
		document.getElementById('structure-filter')?.focus()
	}
</script>

<svelte:window onkeydown={onFindKeydown} />

<div class="flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash">
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
			ondone={() => (editing = false)}
		/>
	{:else if loading && !structure}
		<div class="flex min-h-0 flex-1 flex-col" aria-busy="true">
			<div
				class="flex h-control shrink-0 items-center gap-2 border-b border-line bg-elevated px-2.5"
			>
				<div class="size-6 animate-pulse rounded-md bg-input"></div>
				<div class="h-3 w-36 animate-pulse rounded-sm bg-input"></div>
				<div
					class="ml-auto h-control-sm w-52 animate-pulse rounded-md bg-input"
				></div>
			</div>
			<div
				class="flex h-control-sm shrink-0 items-stretch overflow-hidden border-b border-line bg-panel"
			>
				{#each [1, 2, 3, 4] as i (i)}
					<div
						class="flex items-center gap-1.5 border-r border-line px-2.5"
						style="animation-delay: {i * 40}ms"
					>
						<div class="h-3 w-16 animate-pulse rounded-sm bg-input"></div>
					</div>
				{/each}
			</div>
			<div class="min-h-0 flex-1">
				<div class="flex gap-3 border-b border-line bg-panel px-2.5 py-1.5">
					<div class="h-3 w-8 animate-pulse rounded-sm bg-input"></div>
					<div class="h-3 w-24 animate-pulse rounded-sm bg-input"></div>
					<div class="h-3 w-20 animate-pulse rounded-sm bg-input/70"></div>
					<div class="h-3 flex-1 animate-pulse rounded-sm bg-input/50"></div>
				</div>
				{#each [1, 2, 3, 4, 5, 6, 7, 8] as r (r)}
					<div
						class="flex min-h-grid-row items-center gap-3 border-b border-line px-2.5"
					>
						<div class="h-3 w-6 animate-pulse rounded-sm bg-input/60"></div>
						<div class="h-3 w-28 animate-pulse rounded-sm bg-input"></div>
						<div class="h-3 w-20 animate-pulse rounded-sm bg-input/70"></div>
						<div class="h-3 w-12 animate-pulse rounded-sm bg-input/50"></div>
					</div>
				{/each}
			</div>
			<StatusBar ariaLabel="Structure status">
				{#snippet left()}
					<span class="text-fg-muted">{loadingPun}</span>
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
		<header
			class="relative flex shrink-0 items-center gap-2 border-b border-line bg-elevated px-2.5 py-2"
		>
			<span class="absolute inset-x-0 top-0 h-0.5 bg-conn" aria-hidden="true"
			></span>
			<span
				class="grid size-6 shrink-0 place-items-center rounded-md bg-input"
				aria-hidden="true"
			>
				<CatalogIcon
					name={kindIcon}
					tone={iconToneForName(kindIcon)}
					class="size-3.5"
				/>
			</span>
			<div class="flex min-w-0 flex-1 items-center gap-1.5">
				<h1
					class="m-0 min-w-0 truncate font-mono text-label font-semibold text-fg"
					title={structure.title}
				>
					{structure.title}
				</h1>
				{#if structure.objectKind}
					<Pill tone="neutral" uppercase>{structure.objectKind}</Pill>
				{/if}
			</div>
			{#if editable}
				<Button variant="secondary" size="sm" onclick={() => (editing = true)}>Edit</Button>
				<Button variant="ghost" size="sm" onclick={() => (comparing = true)}>Compare</Button>
			{/if}
			<div class="w-40 shrink-0">
				<ExplorerSearch
					bind:value={filter}
					id="structure-filter"
					placeholder={section
						? `Filter ${section.title.toLowerCase()}…`
						: 'Filter…'}
					label={section
						? `Filter ${section.title.toLowerCase()}`
						: 'Filter structure'}
				/>
			</div>
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
				class="scrollbar-none flex h-control-sm shrink-0 items-stretch overflow-x-auto border-b border-line bg-panel"
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
							'relative inline-flex shrink-0 items-center gap-1.5 border-r border-line px-2.5 text-label font-medium transition-colors',
							active
								? 'bg-app-wash text-fg'
								: 'bg-transparent text-fg-secondary hover:bg-hover hover:text-fg',
						)}
						onclick={() => selectSection(sec.id)}
						onkeydown={onSectionKeydown}
					>
						{#if active}
							<span
								class="absolute inset-x-0 top-0 h-0.5 bg-accent"
								aria-hidden="true"
							></span>
						{/if}
						<Icon
							name={sectionIcon(sec.id)}
							class={cn('size-3.5', active ? 'text-accent' : 'text-fg-muted')}
						/>
						{sec.title}
						<span
							class="rounded-sm bg-input px-1 font-mono text-micro text-fg-muted tabular-nums"
						>
							{sec.rows.length}
						</span>
					</button>
				{/each}
			</div>
		{/if}

		<div class="flex min-h-0 min-w-0 flex-1 flex-col">
			{#if section}
				{#if section.rows.length === 0}
					<div
						class="flex flex-1 flex-col items-center justify-center px-6 py-8 text-center"
					>
						<span
							class="mb-2 grid size-8 place-items-center rounded-lg bg-input"
							aria-hidden="true"
						>
							<Icon
								name={sectionIcon(section.id)}
								class="size-4 text-fg-muted"
							/>
						</span>
						<p class="m-0 text-label font-medium text-fg">
							No {section.title.toLowerCase()}
						</p>
						<p class="mt-1 mb-0 max-w-prose-ui text-caption text-fg-muted">
							{section.emptyMessage ||
								`No ${section.title.toLowerCase()} on this object.`}
						</p>
					</div>
				{:else if filteredRows.length === 0}
					<div
						class="flex flex-1 flex-col items-center justify-center px-6 py-8 text-center"
					>
						<span
							class="mb-2 grid size-8 place-items-center rounded-lg bg-input"
							aria-hidden="true"
						>
							<Icon name="search" class="size-4 text-fg-muted" />
						</span>
						<p class="m-0 text-label font-medium text-fg">No matches</p>
						<p class="mt-1 mb-3 text-caption text-fg-muted">
							Nothing in {section.title.toLowerCase()} matches “{filter.trim()}”
						</p>
						<Button
							variant="ghost"
							size="sm"
							onclick={() => {
								filter = ''
							}}
						>
							Clear filter
						</Button>
					</div>
				{:else}
					<div class="min-h-0 flex-1 overflow-auto">
						<table class="w-full border-collapse text-left">
							<thead class="sticky top-0 z-10">
								<tr class="border-b border-line bg-panel">
									<th
										class="w-8 px-2 py-1.5 text-center text-micro font-medium text-fg-muted"
										scope="col"
									>
										#
									</th>
									{#each section.columns as col (col)}
										<th
											class="px-2.5 py-1.5 text-label font-semibold text-fg-muted"
											scope="col"
										>
											{col}
										</th>
									{/each}
									<th class="w-9 px-1 py-1.5" scope="col">
										<span class="sr-only">Copy</span>
									</th>
								</tr>
							</thead>
							<tbody>
								{#each filteredRows as row, ri (ri)}
									<tr
										class={cn(
											'group/row border-b border-line last:border-b-0',
											'hover:bg-hover/80',
											ri % 2 === 1 && 'bg-input/30',
										)}
									>
										<td
											class="min-h-grid-row px-2 py-1.5 text-center font-mono text-micro text-fg-muted tabular-nums"
										>
											{ri + 1}
										</td>
										{#each row as cell, ci (ci)}
											{@const header = section.columns[ci] ?? ''}
											{@const long = isLong(cell)}
											{@const expanded = expandedCells.has(cellKey(ri, ci))}
											<td
												class={cn(
													'max-w-md px-2.5 py-1.5 align-middle text-caption text-fg',
													ci === 0 && 'font-medium',
												)}
											>
												{#if isEmptyish(cell)}
													<span class="text-fg-muted">—</span>
												{:else if isNullableHeader(header) && (isYes(cell) || isNo(cell))}
													{#if isYes(cell)}
														<span
															class="inline-flex items-center"
															title="Nullable"
														>
															<Icon
																name="check"
																class="size-3.5 text-fg-secondary"
															/>
															<span class="sr-only">Nullable</span>
														</span>
													{:else}
														<span class="text-fg-muted" title="Not nullable"
															>—</span
														>
														<span class="sr-only">Not nullable</span>
													{/if}
												{:else if isBoolHeader(header) && (isYes(cell) || isNo(cell))}
													{#if isYes(cell)}
														<span
															class={cn(
																'inline-flex rounded-sm border px-1.5 py-px font-mono text-micro font-semibold uppercase tracking-wide',
																isPkHeader(header)
																	? 'border-accent/35 bg-accent-muted text-accent'
																	: 'border-success/35 bg-success/15 text-success',
															)}
														>
															{isPkHeader(header) ? 'pk' : 'yes'}
														</span>
													{:else}
														<span class="text-fg-muted">—</span>
														<span class="sr-only">No</span>
													{/if}
												{:else if isTypeHeader(header)}
													<code
														class="rounded-sm border border-line bg-input px-1.5 py-0.5 font-mono text-caption text-fg-secondary"
													>
														{cell}
													</code>
												{:else if long}
													<div class="flex min-w-0 items-start gap-1 py-1">
														<pre
															class={cn(
																'm-0 min-w-0 flex-1 overflow-x-auto rounded-sm border border-line bg-input/60 p-1.5 font-mono text-caption leading-relaxed text-fg whitespace-pre-wrap break-all',
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
																class={cn(
																	'size-3.5 transition-transform duration-150',
																	expanded && 'rotate-180',
																)}
															/>
														</IconButton>
													</div>
												{:else if ci === 0}
													<span
														class="font-mono text-label font-medium text-fg"
														title={cell}>{cell}</span
													>
												{:else}
													<span
														class="font-mono text-caption text-fg-secondary"
														title={cell}>{cell}</span
													>
												{/if}
											</td>
										{/each}
										<td class="px-1 align-middle">
											<div
												class="opacity-0 transition-opacity group-hover/row:opacity-100 focus-within:opacity-100"
											>
												<IconButton
													label="Copy row"
													size="sm"
													variant="quiet"
													onclick={() =>
														void copyWithToast('Row copied', row.join('\t'))}
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

		<StatusBar class="pl-1!" ariaLabel="Structure status">
			{#snippet left()}
				{#if onopentable}
					<Button
						variant="ghost"
						size="sm"
						class="px-1.5!"
						onclick={onopentable}
						title="Open {openTableLabel.toLowerCase()} data in a new tab"
					>
						<Icon name={openTableIcon} class="size-3.5" />
						{openTableLabel}
					</Button>
				{/if}
				{#if lastFetchedAt != null}
					<span
						class="inline-flex min-w-0 items-center gap-1 truncate text-caption text-fg-muted"
						title={new Date(lastFetchedAt).toLocaleString()}
					>
						<Icon name="history" class="size-3.5 shrink-0" />
						<span class="truncate tabular-nums"
							>{fetchedLabel(lastFetchedAt, now)}</span
						>
					</span>
				{/if}
			{/snippet}
			{#snippet right()}
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
		<div
			class="flex flex-1 flex-col items-center justify-center px-6 py-8 text-center"
		>
			<span
				class="mb-2 grid size-8 place-items-center rounded-lg bg-input"
				aria-hidden="true"
			>
				<Icon name="structure" class="size-4 text-info" />
			</span>
			<p class="m-0 text-label font-medium text-fg">Object structure</p>
			<p class="mt-1 mb-0 max-w-prose-ui text-caption text-fg-muted">
				Right-click a table or collection and choose View structure.
			</p>
		</div>
	{/if}
</div>
