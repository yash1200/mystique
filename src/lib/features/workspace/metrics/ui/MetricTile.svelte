<script lang="ts">
	import type { MetricValue } from '#core/contracts'
	import { cn } from '#core/util'
	import { formatMetric, formatMetricHint } from '../format'
	import { metricOccupancy, metricTone } from '../presentation'

	interface Props {
		metric: MetricValue
		class?: string
	}

	let { metric, class: className = '' }: Props = $props()

	const hint = $derived(formatMetricHint(metric))
	const fill = $derived(metricOccupancy(metric))
	const tone = $derived(metricTone(metric))
</script>

<div
	class={cn(
		'flex min-h-28 flex-col justify-between rounded-xl border border-line bg-raised px-4 py-3',
		!metric.available && 'opacity-80',
		className,
	)}
>
	<div class="text-caption font-medium text-fg-muted">{metric.label}</div>
	<div class="mt-3">
		<p
			class={cn(
				'm-0 font-mono text-display font-semibold tracking-tight tabular-nums',
				tone === 'warn' ? 'text-warning' : 'text-fg',
			)}
		>
			{formatMetric(metric)}
		</p>
		{#if hint}
			<p class="mt-1 text-caption text-fg-muted">{hint}</p>
		{/if}
	</div>
	{#if fill != null}
		<div
			class="mt-3 h-0.5 overflow-hidden rounded-full bg-line"
			aria-hidden="true"
		>
			<div
				class={cn(
					'h-full rounded-full',
					tone === 'warn' ? 'bg-warning' : 'bg-accent',
				)}
				style="width: {Math.round(fill * 100)}%"
			></div>
		</div>
	{/if}
</div>
