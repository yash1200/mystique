<script lang="ts">
	import { Input, Segmented, Select, type SelectOption } from '$core/ui'
	import type { MetricsPane } from '../types'

	interface Props {
		pane: MetricsPane
		minDurationMs: number
		sessionFilter: string
		slowFilter: string
		onpane: (pane: MetricsPane) => void
		onduration: (ms: number) => void
	}

	let {
		pane,
		minDurationMs,
		sessionFilter = $bindable(''),
		slowFilter = $bindable(''),
		onpane,
		onduration,
	}: Props = $props()

	const paneOptions = [
		{ value: 'overview', label: 'Overview' },
		{ value: 'sessions', label: 'Connections' },
		{ value: 'slow', label: 'Slow queries' },
	]

	const durationOptions: SelectOption[] = [
		{ value: '500', label: '500 ms' },
		{ value: '1000', label: '1 s' },
		{ value: '5000', label: '5 s' },
		{ value: '10000', label: '10 s' },
	]
</script>

<div
	class="flex shrink-0 flex-wrap items-center gap-2 border-b border-line px-3 py-2"
>
	<Segmented
		value={pane}
		options={paneOptions}
		label="Metrics section"
		onchange={(v) => onpane(v as MetricsPane)}
	/>
	{#if pane === 'slow'}
		<div class="w-28">
			<Select
				value={String(minDurationMs)}
				options={durationOptions}
				inputSize="sm"
				onchange={(v) => onduration(Number(v) || 1000)}
			/>
		</div>
		<div class="min-w-40 flex-1">
			<Input
				inputSize="sm"
				placeholder="Filter queries"
				aria-label="Filter queries"
				bind:value={slowFilter}
			/>
		</div>
	{/if}
	{#if pane === 'sessions'}
		<div class="min-w-40 flex-1">
			<Input
				inputSize="sm"
				placeholder="Filter connections"
				aria-label="Filter connections"
				bind:value={sessionFilter}
			/>
		</div>
	{/if}
</div>
