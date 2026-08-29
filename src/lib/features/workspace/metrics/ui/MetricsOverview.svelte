<script lang="ts">
	import type { ServerMetricsSnapshot } from '$core/contracts'
	import { Alert } from '$core/ui'
	import { formatMetric } from '../format'
	import MetricTile from './MetricTile.svelte'

	interface Props {
		snapshot: ServerMetricsSnapshot
		error?: string | null
	}

	let { snapshot, error = null }: Props = $props()

	const tiles = $derived([
		{
			metric: snapshot.overview.connections,
			span: 'col-span-12 sm:col-span-6 lg:col-span-5',
		},
		{
			metric: snapshot.overview.cacheHitRatio,
			span: 'col-span-6 lg:col-span-3',
		},
		{
			metric: snapshot.overview.databaseSize,
			span: 'col-span-6 lg:col-span-4',
		},
		{ metric: snapshot.overview.uptime, span: 'col-span-12 sm:col-span-7' },
		{
			metric: snapshot.overview.activeQueries,
			span: 'col-span-12 sm:col-span-5',
		},
	])
</script>

{#if error}
	<div class="mb-3">
		<Alert tone="warning">{error}</Alert>
	</div>
{/if}

<div class="grid grid-cols-12 gap-3">
	{#each tiles as tile (tile.metric.key)}
		<MetricTile metric={tile.metric} class={tile.span} />
	{/each}
</div>

{#if snapshot.overview.extras.length}
	<dl
		class="mt-6 grid grid-cols-2 gap-x-6 gap-y-3 sm:grid-cols-3 lg:grid-cols-4"
	>
		{#each snapshot.overview.extras as extra (extra.key)}
			<div>
				<dt class="text-caption text-fg-muted">{extra.label}</dt>
				<dd class="m-0 font-mono text-body tabular-nums text-fg">
					{formatMetric(extra)}
				</dd>
			</div>
		{/each}
	</dl>
{/if}

{#if snapshot.capabilities.notes.length}
	<div class="mt-5 space-y-2">
		{#each snapshot.capabilities.notes as note (note)}
			<Alert tone="info">{note}</Alert>
		{/each}
	</div>
{/if}
