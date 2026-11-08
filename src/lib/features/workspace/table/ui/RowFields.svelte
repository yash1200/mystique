<script lang="ts">
	import type { ColumnDef } from '#core/contracts'
	import { Input, Select } from '#core/ui'
	import { cn } from '#core/util'
	import {
		selectDraftToValue,
		selectOptionsForColumn,
		usesSelectEditor,
		valueToSelectDraft,
	} from '#features/workspace/data-grid'
	import { buildRowJson } from '#features/workspace/data-grid'

	interface Props {
		tableTitle: string
		columns: ColumnDef[]
		row: Record<string, unknown>
		rowIndex: number
		savedRow: Record<string, unknown> | null
		onchange?: (column: string, value: unknown) => void
		/** Bumps when the saved baseline moves; drafts reseed. */
		baselineVersion: number
		readOnly: boolean
		jsonText?: string
		dirtyCount?: number
		statusLabel?: string
	}

	let {
		tableTitle,
		columns,
		row,
		rowIndex,
		savedRow,
		onchange,
		baselineVersion,
		readOnly,
		jsonText = $bindable(''),
		dirtyCount = $bindable(0),
		statusLabel = $bindable(''),
	}: Props = $props()

	function formatForEdit(col: ColumnDef, value: unknown): string {
		if (usesSelectEditor(col, value)) return valueToSelectDraft(value)
		if (value === null || value === undefined) return ''
		if (typeof value === 'boolean') return value ? 'true' : 'false'
		if (typeof value === 'object') return JSON.stringify(value, null, 2)
		return String(value)
	}

	function seedDrafts(): Record<string, string> {
		const next: Record<string, string> = {}
		for (const col of columns) {
			next[col.name] = formatForEdit(col, row[col.name])
		}
		return next
	}

	let drafts = $state<Record<string, string>>(seedDrafts())
	let focusedCol = $state<string | null>(null)
	let query = $state('')
	let lastBaselineKey = $state('')

	const saved = $derived(savedRow ?? row)

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

	function currentValue(col: ColumnDef): unknown {
		const previous = row[col.name]
		const draft = drafts[col.name]
		if (draft == null) return previous ?? null
		if (draft === formatForEdit(col, previous)) return previous ?? null
		return parseDraft(col, draft, previous)
	}

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

	const computedDirty = $derived.by(() => {
		let n = 0
		for (const col of columns) {
			if ((drafts[col.name] ?? '') !== formatForEdit(col, saved[col.name])) {
				n += 1
			}
		}
		return n
	})

	const computedJson = $derived.by(() => {
		const values: Record<string, unknown> = {}
		for (const col of columns) {
			values[col.name] = currentValue(col)
		}
		return buildRowJson(columns, values)
	})

	const computedStatus = $derived.by(() => {
		const q = query.trim()
		if (q) return `${visibleColumns.length} of ${columns.length}`
		return `${columns.length} column${columns.length === 1 ? '' : 's'}`
	})

	$effect(() => {
		jsonText = computedJson
		dirtyCount = computedDirty
		statusLabel = computedStatus
	})

	$effect(() => {
		const key = `${tableTitle}::${rowIndex}::${baselineVersion}`
		if (key === lastBaselineKey) return
		lastBaselineKey = key
		drafts = seedDrafts()
		focusedCol = null
	})

	$effect(() => {
		if (focusedCol) return
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

	export function commitFocused() {
		if (focusedCol) commitField(focusedCol)
	}

	function isDirty(colName: string): boolean {
		const col = columns.find((c) => c.name === colName)
		if (!col) return false
		return (drafts[colName] ?? '') !== formatForEdit(col, saved[colName])
	}

	function isEmptyDraft(colName: string): boolean {
		return (drafts[colName] ?? '') === ''
	}

	function commitField(colName: string) {
		if (readOnly) {
			focusedCol = null
			return
		}
		const col = columns.find((c) => c.name === colName)
		if (!col) return
		const previous = row[colName]
		const draft = drafts[colName] ?? ''
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
</script>

<div class="flex h-full min-h-0 flex-col">
	<div class="shrink-0 px-2 py-2">
		<Input
			type="search"
			placeholder="Filter columns…"
			bind:value={query}
			inputSize="sm"
		/>
	</div>
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
					{@const dirty = isDirty(col.name)}
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
</div>
