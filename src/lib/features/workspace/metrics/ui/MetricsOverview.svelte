<script lang="ts">
	import type { ServerMetricsSnapshot } from '#core/contracts'
	import { Alert } from '#core/ui'
	import { formatMetric } from '../format'
	import MetricTile from './MetricTile.svelte'

	interface Props {
		snapshot: ServerMetricsSnapshot
	}

	let { snapshot }: Props = $props()

	const tiles = $derived([
		snapshot.overview.connections,
		snapshot.overview.activeQueries,
		snapshot.overview.cacheHitRatio,
		snapshot.overview.databaseSize,
		snapshot.overview.uptime,
	])
</script>

<div class="min-h-0 flex-1 overflow-auto p-4">
	<div class="stagger flex flex-col gap-6">
		<div class="grid grid-cols-[repeat(auto-fill,minmax(13rem,1fr))] gap-3">
			{#each tiles as metric (metric.key)}
				<MetricTile {metric} />
			{/each}
		</div>

		{#if snapshot.overview.extras.length}
			<section aria-labelledby="metrics-engine-details">
				<h2 id="metrics-engine-details" class="m-0 mb-2 label-caps">Engine details</h2>
				<dl class="m-0 grid overflow-hidden rounded-lg border border-line bg-surface sm:grid-cols-2">
					{#each snapshot.overview.extras as extra (extra.key)}
						<div
							class="-mb-px flex h-9 min-w-0 items-center justify-between gap-4 border-b border-line px-4 sm:odd:border-r"
							title={extra.available ? undefined : (extra.unavailableReason ?? 'Not available')}
						>
							<dt class="min-w-0 truncate text-label text-fg-secondary">{extra.label}</dt>
							<dd class="m-0 shrink-0 font-mono text-data text-fg tabular-nums">{formatMetric(extra)}</dd>
						</div>
					{/each}
				</dl>
			</section>
		{/if}

		{#if snapshot.capabilities.notes.length}
			<div class="flex flex-col gap-2">
				{#each snapshot.capabilities.notes as note (note)}
					<Alert tone="info">{note}</Alert>
				{/each}
			</div>
		{/if}
	</div>
</div>
