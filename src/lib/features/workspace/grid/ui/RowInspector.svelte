<script lang="ts">
	import type { ColumnDef } from '$core/contracts'
	import { Icon, IconButton, Input, Select, StatusBar } from '$core/ui'
	import { cn } from '$core/util'
	import {
		selectDraftToValue,
		selectOptionsForColumn,
		usesSelectEditor,
		valueToSelectDraft,
	} from '../column-editor'
	interface Props {
		tableTitle: string
		columns: ColumnDef[]
		row: Record<string, unknown>
		rowIndex: number
		/** Last saved values for this row (grid originals). */
		savedRow?: Record<string, unknown> | null
		onclose: () => void
		/** Commit a field edit (parsed value). */
		onchange?: (column: string, value: unknown) => void
		/** Bumps on grid save / reset / refresh so drafts re-sync. */
		baselineEpoch?: number
		readOnly?: boolean
	}

	let {
		tableTitle,
		columns,
		row,
		rowIndex,
		savedRow = null,
		onclose,
		onchange,
		baselineEpoch = 0,
		readOnly = false,
	}: Props = $props()

	function seedFromRow() {
		const nextBaseline: Record<string, unknown> = {}
		const nextDrafts: Record<string, string> = {}
		for (const col of columns) {
			nextBaseline[col.name] = row[col.name]
			nextDrafts[col.name] = formatForEdit(col, row[col.name])
		}
		return { nextBaseline, nextDrafts }
	}

	const seeded = seedFromRow()
	/** String drafts shown in inputs. */
	let drafts = $state<Record<string, string>>(seeded.nextDrafts)
	let focusedCol = $state<string | null>(null)

	const saved = $derived(savedRow ?? row)

	function formatForEdit(col: ColumnDef, value: unknown): string {
		if (usesSelectEditor(col, value)) return valueToSelectDraft(value)
		if (value === null || value === undefined) return ''
		if (typeof value === 'boolean') return value ? 'true' : 'false'
		if (typeof value === 'object') return JSON.stringify(value, null, 2)
		return String(value)
	}

	function valuesEqual(a: unknown, b: unknown): boolean {
		if (a === b) return true
		if (a == null && b == null) return true
		if (typeof a === 'object' && typeof b === 'object' && a && b) {
			try {
				return JSON.stringify(a) === JSON.stringify(b)
			} catch {
				return false
			}
		}
		return String(a) === String(b)
	}

	function parseDraft(col: ColumnDef, raw: string, previous: unknown): unknown {
		if (usesSelectEditor(col, previous)) {
			return selectDraftToValue(raw, col, previous)
		}
		const t = raw.trim()
		if (t === '' || t.toUpperCase() === 'NULL') {
			// Keep empty string if that's what the cell already holds.
			return previous === '' ? '' : null
		}
		if (typeof previous === 'boolean') {
			const lower = t.toLowerCase()
			if (lower === 'true' || lower === '1' || lower === 'yes') return true
			if (lower === 'false' || lower === '0' || lower === 'no') return false
		}
		if (typeof previous === 'number') {
			const n = Number(t)
			if (!Number.isNaN(n)) return n
		}
		// Only parse JSON when the cell already holds an object.
		if (
			previous !== null &&
			typeof previous === 'object' &&
			((t.startsWith('{') && t.endsWith('}')) ||
				(t.startsWith('[') && t.endsWith(']')))
		) {
			try {
				return JSON.parse(t)
			} catch {
				/* keep string */
			}
		}
		return raw
	}

	function useTextarea(col: ColumnDef, value: unknown): boolean {
		if (usesSelectEditor(col, value)) return false
		const t = col.type.toLowerCase()
		if (t.includes('json') || t.includes('text') || t.includes('object')) {
			return true
		}
		if (t.includes('varchar') && formatForEdit(col, value).length > 48) {
			return true
		}
		if (typeof value === 'object' && value !== null) return true
		return formatForEdit(col, value).length > 64
	}

	function syncDraftsFromRow() {
		drafts = seedFromRow().nextDrafts
		focusedCol = null
	}

	// New row / table, or save/reset → refresh drafts from the live row.
	// Do not treat the current (possibly dirty) row as the saved baseline.
	let lastBaselineKey = $state('')
	$effect(() => {
		const key = `${tableTitle}::${rowIndex}::${baselineEpoch}`
		if (key === lastBaselineKey) return
		lastBaselineKey = key
		syncDraftsFromRow()
	})

	// Sync drafts from row when grid edits land (skip while typing)
	$effect(() => {
		if (focusedCol) return
		// Depend on row field values
		for (const col of columns) {
			void row[col.name]
		}
		const next: Record<string, string> = {}
		let changed = false
		for (const col of columns) {
			const v = formatForEdit(col, row[col.name])
			next[col.name] = v
			if (drafts[col.name] !== v) changed = true
		}
		if (changed) drafts = next
	})

	function isDirty(colName: string): boolean {
		const col = columns.find((c) => c.name === colName)
		if (!col) return false
		return (drafts[colName] ?? '') !== formatForEdit(col, saved[colName])
	}

	const dirtyCols = $derived.by(() => {
		const next = new Set<string>()
		for (const col of columns) {
			if (isDirty(col.name)) next.add(col.name)
		}
		return next
	})

	function commitField(colName: string) {
		if (readOnly) {
			focusedCol = null
			return
		}
		const col = columns.find((c) => c.name === colName)
		if (!col) return
		const previous = row[colName]
		const draft = drafts[colName] ?? ''
		// Blur with no text change must not coerce types (JSON string → object, "" → null).
		if (draft === formatForEdit(col, previous)) {
			focusedCol = null
			return
		}
		const next = parseDraft(col, draft, previous)
		drafts = { ...drafts, [colName]: formatForEdit(col, next) }
		if (!valuesEqual(next, previous)) {
			onchange?.(colName, next)
		}
		focusedCol = null
	}

	function onFieldKeydown(
		e: KeyboardEvent,
		colName: string,
		multiline: boolean,
	) {
		if (e.key === 'Escape') {
			e.preventDefault()
			const col = columns.find((c) => c.name === colName)
			drafts = {
				...drafts,
				[colName]: col
					? formatForEdit(col, row[colName])
					: String(row[colName] ?? ''),
			}
			focusedCol = null
			;(e.currentTarget as HTMLElement).blur()
			return
		}
		if (e.key === 'Enter' && !multiline && !e.shiftKey) {
			e.preventDefault()
			commitField(colName)
			;(e.currentTarget as HTMLElement).blur()
		}
	}

	const dirtyCount = $derived(dirtyCols.size)

	let query = $state('')

	const visibleColumns = $derived.by(() => {
		const q = query.trim().toLowerCase()
		return columns
			.map((col, i) => ({ col, i }))
			.filter(
				({ col }) =>
					!q ||
					col.name.toLowerCase().includes(q) ||
					col.type.toLowerCase().includes(q),
			)
			.sort((a, b) => Number(!!b.col.pk) - Number(!!a.col.pk) || a.i - b.i)
			.map(({ col }) => col)
	})

	function isEmptyDraft(colName: string): boolean {
		return (drafts[colName] ?? '') === ''
	}

	const filtering = $derived(query.trim().length > 0)
	const countLabel = $derived(
		filtering
			? `${visibleColumns.length} of ${columns.length}`
			: `${columns.length} column${columns.length === 1 ? '' : 's'}`,
	)
</script>

<aside
	class="flex h-full w-80 shrink-0 flex-col border-l border-line bg-panel"
	aria-label="Row detail"
>
	<header
		class="flex shrink-0 items-center gap-1.5 border-b border-line px-3 py-2"
	>
		<div class="min-w-0 flex-1">
			<Input
				type="search"
				placeholder="Filter columns…"
				bind:value={query}
				inputSize="sm"
			/>
		</div>
		<IconButton
			label="Close row inspector"
			size="sm"
			variant="ghost"
			onclick={onclose}
		>
			<Icon name="close" />
		</IconButton>
	</header>

	<div class="min-h-0 flex-1 overflow-auto">
		{#if visibleColumns.length === 0}
			<p class="m-3 text-caption leading-relaxed text-fg-muted">
				{#if query.trim()}
					No columns match “{query.trim()}”.
				{:else}
					This row has no columns.
				{/if}
			</p>
		{:else}
			<ul class="m-0 list-none p-0">
				{#each visibleColumns as col (col.name)}
					{@const raw = row[col.name]}
					{@const dirty = dirtyCols.has(col.name)}
					{@const multiline = useTextarea(col, raw)}
					{@const focused = focusedCol === col.name}
					{@const empty = isEmptyDraft(col.name)}
					<li
						class={cn(
							'px-3 py-2 transition-colors duration-150',
							dirty && 'bg-warning/10',
							focused && !dirty && 'bg-input',
						)}
					>
						<div class="mb-1 flex min-w-0 items-center gap-1.5">
							<span
								class="min-w-0 truncate font-mono text-label font-medium text-fg"
							>
								{col.name}
							</span>
							{#if col.pk}
								<span
									class="shrink-0 rounded-sm border border-accent/35 bg-accent-muted px-1 py-px font-mono text-micro font-semibold text-accent"
									title="Primary key">PK</span
								>
							{/if}
							<span
								class="ml-auto min-w-0 truncate font-mono text-micro text-fg-muted"
								title={col.type}
							>
								{col.type}
							</span>
							{#if dirty}
								<span class="sr-only">Edited</span>
							{/if}
						</div>

						{#if usesSelectEditor(col, raw)}
							{@const options = selectOptionsForColumn(col, raw)}
							<Select
								inputSize="sm"
								class="font-mono"
								value={drafts[col.name] ?? ''}
								{options}
								disabled={readOnly}
								onchange={(v) => {
									drafts = { ...drafts, [col.name]: v }
									commitField(col.name)
								}}
							/>
						{:else if multiline}
							<textarea
								class={cn(
									'm-0 min-h-16 w-full resize-y rounded-md border bg-elevated px-2 py-1.5',
									'font-mono text-label leading-relaxed text-fg',
									'focus:border-focus focus:outline-none focus:ring-2 focus:ring-accent-muted',
									dirty ? 'border-warning/45' : 'border-line',
									empty && 'italic text-fg-muted',
								)}
								rows="3"
								value={drafts[col.name] ?? ''}
								placeholder="NULL"
								aria-label={col.name}
								readonly={readOnly}
								onfocus={() => (focusedCol = col.name)}
								oninput={(e) => {
									drafts = {
										...drafts,
										[col.name]: (e.currentTarget as HTMLTextAreaElement).value,
									}
								}}
								onblur={() => commitField(col.name)}
								onkeydown={(e) => onFieldKeydown(e, col.name, true)}
								spellcheck="false"
							></textarea>
						{:else}
							<input
								class={cn(
									'm-0 h-control-sm w-full rounded-md border bg-elevated px-2',
									'font-mono text-label text-fg',
									'focus:border-focus focus:outline-none focus:ring-2 focus:ring-accent-muted',
									dirty ? 'border-warning/45' : 'border-line',
									empty && 'italic text-fg-muted',
								)}
								value={drafts[col.name] ?? ''}
								placeholder="NULL"
								aria-label={col.name}
								readonly={readOnly}
								onfocus={() => (focusedCol = col.name)}
								oninput={(e) => {
									drafts = {
										...drafts,
										[col.name]: (e.currentTarget as HTMLInputElement).value,
									}
								}}
								onblur={() => commitField(col.name)}
								onkeydown={(e) => onFieldKeydown(e, col.name, false)}
								spellcheck="false"
								autocomplete="off"
							/>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</div>

	<StatusBar ariaLabel="Row inspector status">
		{#snippet left()}
			<span class="font-mono tabular-nums">{countLabel}</span>
		{/snippet}
		{#snippet right()}
			{#if dirtyCount > 0}
				<span class="font-mono tabular-nums text-warning"
					>{dirtyCount} unsaved</span
				>
			{/if}
		{/snippet}
	</StatusBar>
</aside>
