<script lang="ts">
	/**
	 * Object structure — laboratory ledger for columns, indexes, relations, …
	 * Data is plugin-owned; this panel only renders sections.
	 */
	import type { ObjectStructure, StructureSection } from '$lib/api/engines'
	import {
		Button,
		Checkbox,
		EmptyState,
		Icon,
		IconButton,
		Input,
		Pill,
		StatusBar,
	} from '$lib/components/core'
	import { cn } from '$lib/utils/cn'
	import { copyTextToClipboard } from '$lib/utils/exportTable'

	interface Props {
		structure: ObjectStructure | null
		loading?: boolean
		error?: string | null
		onretry?: () => void
	}

	let {
		structure = null,
		loading = false,
		error = null,
		onretry,
	}: Props = $props()

	let activeSection = $state<string | null>(null)
	let filter = $state('')
	let expandedCells = $state<Set<string>>(new Set())
	let copyMsg = $state<string | null>(null)

	$effect(() => {
		// Reset chrome when the object changes.
		void structure?.title
		filter = ''
		expandedCells = new Set()
		copyMsg = null
	})

	$effect(() => {
		const first = structure?.sections?.[0]?.id ?? null
		if (!structure) {
			activeSection = null
			return
		}
		if (
			!activeSection ||
			!structure.sections.some((s) => s.id === activeSection)
		) {
			activeSection = first
		}
	})

	const section = $derived(
		structure?.sections.find((s) => s.id === activeSection) ??
			structure?.sections[0] ??
			null,
	)

	const totalFacts = $derived(
		structure?.sections.reduce((n, s) => n + s.rows.length, 0) ?? 0,
	)

	const filteredRows = $derived.by(() => {
		if (!section) return [] as string[][]
		const q = filter.trim().toLowerCase()
		if (!q) return section.rows
		return section.rows.filter((row) =>
			row.some((cell) => cell.toLowerCase().includes(q)),
		)
	})

	function selectSection(id: string) {
		activeSection = id
		filter = ''
		expandedCells = new Set()
	}

	function cellKey(ri: number, ci: number): string {
		return `${ri}:${ci}`
	}

	function isExpanded(ri: number, ci: number): boolean {
		return expandedCells.has(cellKey(ri, ci))
	}

	function toggleExpand(ri: number, ci: number) {
		const k = cellKey(ri, ci)
		const next = new Set(expandedCells)
		if (next.has(k)) next.delete(k)
		else next.add(k)
		expandedCells = next
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

	function sectionIcon(id: string): string {
		switch (id) {
			case 'columns':
			case 'fields':
				return '∷'
			case 'indexes':
				return '⌘'
			case 'foreign_keys':
			case 'referenced_by':
				return '→'
			case 'triggers':
				return '⚡'
			case 'checks':
				return '✓'
			case 'options':
				return '⚙'
			case 'stats':
				return '∑'
			default:
				return '·'
		}
	}

	async function flashCopy(label: string, text: string) {
		const res = await copyTextToClipboard(text)
		const msg = res.ok
			? label
			: !res.cancelled && 'error' in res
				? res.error
				: null
		copyMsg = msg
		if (msg) {
			setTimeout(() => {
				if (copyMsg === msg) copyMsg = null
			}, 1600)
		}
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
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash">
	{#if loading && !structure}
		<div class="flex min-h-0 flex-1 flex-col">
			<div class="shrink-0 border-b border-line bg-panel px-4 py-3">
				<div class="h-4 w-48 animate-pulse rounded-sm bg-input"></div>
				<div class="mt-2 h-3 w-32 animate-pulse rounded-sm bg-input/80"></div>
			</div>
			<div
				class="flex shrink-0 gap-1.5 overflow-hidden border-b border-line bg-panel px-3 py-2"
			>
				{#each [1, 2, 3, 4, 5] as i (i)}
					<div
						class="h-control-sm w-20 shrink-0 animate-pulse rounded-md bg-input"
						style="animation-delay: {i * 40}ms"
					></div>
				{/each}
			</div>
			<div class="min-w-0 flex-1 space-y-2 p-4">
				<div class="h-3 w-40 animate-pulse rounded-sm bg-input"></div>
				<div class="overflow-hidden rounded-md border border-line bg-elevated">
					{#each [1, 2, 3, 4, 5, 6, 7, 8] as r (r)}
						<div
							class="flex gap-3 border-b border-line px-3 py-2 last:border-b-0"
						>
							<div class="h-3 w-24 animate-pulse rounded-sm bg-input"></div>
							<div class="h-3 w-20 animate-pulse rounded-sm bg-input/70"></div>
							<div
								class="h-3 flex-1 animate-pulse rounded-sm bg-input/50"
							></div>
						</div>
					{/each}
				</div>
			</div>
			<div
				class="flex min-h-statusbar shrink-0 items-center border-t border-line bg-panel px-3 text-caption text-fg-muted"
			>
				Reading structure from the engine…
			</div>
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
		<!-- Identity header -->
		<header class="relative shrink-0 border-b border-line bg-panel">
			<span class="absolute inset-y-0 left-0 w-0.5 bg-conn" aria-hidden="true"
			></span>
			<div class="flex items-start gap-3 px-4 py-3 pl-4">
				<div class="min-w-0 flex-1">
					<div class="flex flex-wrap items-center gap-2">
						<h1
							class="m-0 min-w-0 truncate font-mono text-body-lg font-semibold tracking-tight text-fg"
							title={structure.title}
						>
							{structure.title}
						</h1>
						{#if structure.objectKind}
							<Pill tone="neutral" uppercase>{structure.objectKind}</Pill>
						{/if}
						{#if loading}
							<span class="text-caption text-fg-muted">Refreshing…</span>
						{/if}
					</div>
					<div
						class="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-caption text-fg-muted"
					>
						{#if structure.subtitle}
							<span>{structure.subtitle}</span>
							<span aria-hidden="true" class="text-fg-muted">·</span>
						{/if}
						<span class="font-mono tabular-nums">
							{structure.sections.length} section{structure.sections.length ===
							1
								? ''
								: 's'}
						</span>
						<span aria-hidden="true" class="text-fg-muted">·</span>
						<span class="font-mono tabular-nums">
							{totalFacts} fact{totalFacts === 1 ? '' : 's'}
						</span>
						{#if section}
							<span aria-hidden="true" class="text-fg-muted">·</span>
							<span>{section.title}</span>
						{/if}
					</div>
					{#if error}
						<p class="mt-1.5 mb-0 text-caption text-danger" role="alert">
							{error}
							{#if onretry}
								<button
									type="button"
									class="ml-1 border-none bg-transparent p-0 text-accent underline"
									onclick={onretry}
								>
									Retry
								</button>
							{/if}
						</p>
					{/if}
				</div>
				{#if section && section.rows.length > 0}
					<div class="flex shrink-0 items-center gap-1">
						<Button
							variant="ghost"
							size="sm"
							title="Copy this section as TSV"
							onclick={() =>
								void flashCopy('Section copied', sectionAsTsv(section))}
						>
							Copy section
						</Button>
					</div>
				{/if}
			</div>
		</header>

		<!-- Horizontal catalog (avoids a third vertical list panel) -->
		{#if structure.sections.length > 0}
			<div
				class="scrollbar-none flex shrink-0 items-center gap-1 overflow-x-auto border-b border-line bg-panel px-3 py-1.5"
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
							'inline-flex h-control-sm shrink-0 items-center gap-1.5 rounded-md border px-2.5 text-label font-medium transition-colors',
							active
								? 'border-accent/40 bg-accent-muted text-accent'
								: 'border-transparent bg-transparent text-fg-secondary hover:bg-hover hover:text-fg',
						)}
						onclick={() => selectSection(sec.id)}
						onkeydown={onSectionKeydown}
					>
						<span
							class={cn(
								'font-mono text-micro',
								active ? 'text-accent' : 'text-fg-muted',
							)}
							aria-hidden="true"
						>
							{sectionIcon(sec.id)}
						</span>
						{sec.title}
						<span
							class={cn(
								'font-mono text-micro tabular-nums',
								active ? 'text-accent' : 'text-fg-muted',
							)}
						>
							{sec.rows.length}
						</span>
					</button>
				{/each}
			</div>
		{/if}

		<div class="flex min-h-0 min-w-0 flex-1 flex-col">
			{#if section}
				<div
					class="flex shrink-0 flex-wrap items-center gap-2 border-b border-line bg-panel px-3 py-2"
				>
					<div class="min-w-0 flex-1">
						<div class="text-label font-semibold text-fg">{section.title}</div>
						{#if section.description}
							<div class="mt-0.5 text-caption text-fg-muted">
								{section.description}
							</div>
						{/if}
					</div>
					{#if section.rows.length > 4}
						<div class="w-48 shrink-0">
							<Input
								type="search"
								placeholder="Filter {section.title.toLowerCase()}…"
								bind:value={filter}
								inputSize="sm"
							/>
						</div>
					{/if}
				</div>

				<div class="min-h-0 flex-1 overflow-auto p-3">
					{#if section.rows.length === 0}
						<div
							class="flex min-h-40 flex-col items-center justify-center rounded-md border border-dashed border-line bg-elevated/60 px-4 py-8 text-center"
						>
							<p class="m-0 text-label font-medium text-fg-secondary">
								Nothing here
							</p>
							<p class="mt-1 mb-0 max-w-prose-ui text-caption text-fg-muted">
								{section.emptyMessage ||
									`No ${section.title.toLowerCase()} on this object.`}
							</p>
						</div>
					{:else if filteredRows.length === 0}
						<div
							class="flex min-h-32 flex-col items-center justify-center rounded-md border border-dashed border-line bg-elevated/60 px-4 py-6 text-center"
						>
							<p class="m-0 text-label text-fg-secondary">
								No matches for “{filter.trim()}”
							</p>
							<button
								type="button"
								class="mt-2 border-none bg-transparent text-caption text-accent underline"
								onclick={() => {
									filter = ''
								}}
							>
								Clear filter
							</button>
						</div>
					{:else}
						<div
							class="overflow-hidden rounded-md border border-line bg-elevated shadow-sm"
						>
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
												class="px-2.5 py-1.5 text-label font-semibold tracking-wide text-fg-muted"
												scope="col"
											>
												{col}
											</th>
										{/each}
										<th class="w-10 px-1 py-1.5" scope="col">
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
												class="px-2 py-1.5 text-center font-mono text-micro text-fg-muted tabular-nums"
											>
												{ri + 1}
											</td>
											{#each row as cell, ci (ci)}
												{@const header = section.columns[ci] ?? ''}
												{@const long = isLong(cell)}
												{@const expanded = isExpanded(ri, ci)}
												<td
													class={cn(
														'max-w-md px-2.5 py-1.5 align-top text-caption text-fg',
														ci === 0 && 'font-medium',
													)}
												>
													{#if isEmptyish(cell)}
														<span class="text-fg-muted">—</span>
													{:else if isNullableHeader(header) && (isYes(cell) || isNo(cell))}
														<span
															class="inline-flex"
															title={isYes(cell) ? 'Nullable' : 'Not nullable'}
														>
															<Checkbox
																checked={isYes(cell)}
																disabled
																aria-label={isYes(cell)
																	? 'Nullable'
																	: 'Not nullable'}
																class="cursor-default opacity-100"
															/>
														</span>
													{:else if isBoolHeader(header) && (isYes(cell) || isNo(cell))}
														{#if isYes(cell)}
															<span
																class={cn(
																	'inline-flex rounded-sm border px-1.5 py-px font-mono text-micro font-semibold uppercase tracking-wide',
																	header.toLowerCase() === 'pk' ||
																		header.toLowerCase() === 'primary' ||
																		header.toLowerCase() === 'id'
																		? 'border-accent/35 bg-accent-muted text-accent'
																		: 'border-success/35 bg-success/15 text-success',
																)}
															>
																{header.toLowerCase() === 'pk' ||
																header.toLowerCase() === 'primary'
																	? 'pk'
																	: 'yes'}
															</span>
														{:else}
															<span
																class="inline-flex rounded-sm border border-line bg-input px-1.5 py-px font-mono text-micro text-fg-muted uppercase tracking-wide"
															>
																no
															</span>
														{/if}
													{:else if isTypeHeader(header)}
														<code
															class="rounded-sm border border-line bg-input px-1.5 py-0.5 font-mono text-caption text-fg-secondary"
														>
															{cell}
														</code>
													{:else if long}
														<div class="min-w-0">
															<pre
																class={cn(
																	'm-0 max-w-xl overflow-x-auto rounded-sm border border-line bg-input/60 p-1.5 font-mono text-caption leading-relaxed text-fg whitespace-pre-wrap break-all',
																	!expanded && 'line-clamp-3',
																)}>{cell}</pre>
															<button
																type="button"
																class="mt-1 border-none bg-transparent p-0 text-micro text-accent underline"
																onclick={() => toggleExpand(ri, ci)}
															>
																{expanded ? 'Collapse' : 'Expand'}
															</button>
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
											<td class="px-1 py-1 align-top">
												<div
													class="opacity-0 transition-opacity group-hover/row:opacity-100 focus-within:opacity-100"
												>
													<IconButton
														label="Copy row"
														size="sm"
														variant="ghost"
														onclick={() =>
															void flashCopy('Row copied', row.join('\t'))}
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
				</div>
			{:else}
				<div class="flex flex-1 items-center justify-center p-8">
					<p class="m-0 text-caption text-fg-muted">No structure sections.</p>
				</div>
			{/if}
		</div>

		<StatusBar ariaLabel="Structure status">
			{#snippet left()}
				<span class="min-w-0 truncate">
					<span class="font-mono text-fg-secondary">{structure.title}</span>
					{#if section}
						<span class="text-fg-muted" aria-hidden="true"> · </span>
						<span class="text-fg-secondary">{section.title}</span>
						<span class="text-fg-muted" aria-hidden="true"> · </span>
						<span class="font-mono tabular-nums text-fg-muted">
							{filter.trim()
								? `${filteredRows.length} of ${section.rows.length}`
								: section.rows.length}
						</span>
					{/if}
				</span>
			{/snippet}
			{#snippet right()}
				{#if copyMsg}
					<span class="text-success">{copyMsg}</span>
				{:else if onretry}
					<button
						type="button"
						class="border-none bg-transparent p-0 text-caption text-fg-muted hover:text-accent"
						onclick={onretry}
						title="Reload structure from the engine"
					>
						Refresh
					</button>
				{/if}
			{/snippet}
		</StatusBar>
	{:else}
		<div class="flex flex-1 items-center justify-center p-8">
			<EmptyState
				title="Object structure"
				description="Right-click a table or collection and choose View structure to inspect columns, indexes, and related metadata."
			/>
		</div>
	{/if}
</div>
