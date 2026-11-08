<script lang="ts" module>
	type InspectorView = 'fields' | 'json'
	let persistedView: InspectorView = 'fields'
</script>

<script lang="ts">
	import type { ColumnDef } from '#core/contracts'
	import { Icon, IconButton, Segmented, StatusBar } from '#core/ui'
	import { cn, copyTextToClipboard } from '#core/util'
	import { onDestroy } from 'svelte'
	import { buildRowJson, JsonHighlightEditor } from '#features/workspace/data-grid'
	import RowFields from './RowFields.svelte'

	interface Props {
		tableTitle: string
		columns: ColumnDef[]
		row: Record<string, unknown>
		rowIndex: number
		savedRow?: Record<string, unknown> | null
		onclose: () => void
		onchange?: (column: string, value: unknown) => void
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

	const VIEW_OPTIONS = [
		{ value: 'fields', label: 'Fields' },
		{ value: 'json', label: 'JSON' },
	]

	let view = $state<InspectorView>(persistedView)
	let copied = $state(false)
	let copyTimer: ReturnType<typeof setTimeout> | null = null
	let fields = $state<{ commitFocused: () => void } | null>(null)
	let fieldsJson = $state('')
	let dirtyCount = $state(0)
	let fieldsStatus = $state('')

	onDestroy(() => {
		if (copyTimer) clearTimeout(copyTimer)
	})

	const jsonText = $derived(
		view === 'fields' && fieldsJson
			? fieldsJson
			: buildRowJson(columns, row),
	)
	const countLabel = $derived(
		view === 'json'
			? `${columns.length} ${columns.length === 1 ? 'key' : 'keys'}`
			: fieldsStatus,
	)

	function setView(next: string) {
		if (next !== 'fields' && next !== 'json') return
		if (next === view) return
		fields?.commitFocused()
		view = next
		persistedView = next
	}

	async function copyJson() {
		const res = await copyTextToClipboard(jsonText)
		if (!res.ok) return
		copied = true
		if (copyTimer) clearTimeout(copyTimer)
		copyTimer = setTimeout(() => {
			copied = false
			copyTimer = null
		}, 1200)
	}
</script>

<aside
	class="flex h-full w-80 shrink-0 flex-col border-l border-line bg-panel"
	aria-label="Row details"
>
	<header class="shrink-0 border-b border-line">
		<div class="flex items-center gap-1 px-3 pt-3 pb-2">
			<h2
				class="m-0 min-w-0 flex-1 truncate text-body-lg font-semibold tracking-tight text-fg"
			>
				Row details
			</h2>
			<div
				class="flex shrink-0 items-center gap-0.5"
				role="group"
				aria-label="Row details actions"
			>
				<IconButton
					label={copied ? 'Copied' : 'Copy as JSON'}
					size="sm"
					variant="quiet"
					onclick={() => void copyJson()}
				>
					<Icon name={copied ? 'check' : 'copy'} />
				</IconButton>
				<IconButton
					label="Close row details"
					size="sm"
					variant="quiet"
					onclick={onclose}
				>
					<Icon name="close" />
				</IconButton>
			</div>
		</div>
		<div class="px-2 pb-2">
			<Segmented
				label="Row view"
				value={view}
				options={VIEW_OPTIONS}
				onchange={setView}
				class="w-full [&_button]:flex-1"
			/>
		</div>
	</header>

	<div
		class={cn(
			'min-h-0 flex-1',
			view === 'json' ? 'flex flex-col overflow-hidden' : 'overflow-hidden',
		)}
	>
		{#if view === 'json'}
			<JsonHighlightEditor value={jsonText} readonly fill />
		{:else}
			<RowFields
				bind:this={fields}
				{tableTitle}
				{columns}
				{row}
				{rowIndex}
				{savedRow}
				{onchange}
				{baselineEpoch}
				{readOnly}
				bind:jsonText={fieldsJson}
				bind:dirtyCount
				bind:statusLabel={fieldsStatus}
			/>
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
