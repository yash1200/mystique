<script lang="ts">
	import type { MetricValue } from '#core/contracts'
	import { cn } from '#core/util'
	import { formatMetric, formatMetricHint } from '../format'
	import { metricOccupancy, metricWarns } from '../presentation'

	interface Props {
		metric: MetricValue
	}

	let { metric }: Props = $props()

	const hint = $derived(formatMetricHint(metric))
	const fill = $derived(metricOccupancy(metric))
	const warns = $derived(metricWarns(metric))
</script>

<div class="flex min-h-28 flex-col rounded-lg border border-line bg-surface px-4 py-3.5">
	<p class="m-0 label-caps">{metric.label}</p>
	<p
		class={cn(
			'm-0 mt-2.5 font-mono text-title font-semibold tracking-tight tabular-nums',
			!metric.available ? 'text-fg-muted' : warns ? 'text-danger' : 'text-fg',
		)}
	>
		{formatMetric(metric)}
	</p>
	{#if hint}
		<p class="m-0 mt-0.5 truncate font-mono text-caption text-fg-muted" title={hint}>{hint}</p>
	{/if}
	{#if fill != null}
		<div class="mt-auto pt-3" aria-hidden="true">
			<div class="h-1 overflow-hidden rounded-full bg-field">
				<div
					class={cn('h-full rounded-full', warns ? 'bg-danger' : 'bg-accent')}
					style="width: {Math.round(fill * 100)}%"
				></div>
			</div>
		</div>
	{/if}
</div>
