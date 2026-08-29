<script lang="ts">
	import type { ServerMetricsSnapshot } from '$core/contracts'
	import { StatusBar } from '$core/ui'
	import { fetchedLabel } from '$core/util'

	interface Props {
		fetchedAt: number | null
		now: number
		live: boolean
		snapshot: ServerMetricsSnapshot | null
	}

	let { fetchedAt, now, live, snapshot }: Props = $props()
</script>

<StatusBar ariaLabel="Metrics status">
	{#snippet left()}
		<span>
			{#if fetchedAt}
				Collected {fetchedLabel(fetchedAt, now).toLowerCase()}
			{:else}
				Waiting for first sample
			{/if}
		</span>
		{#if live}
			<span class="text-fg-muted"> · live</span>
		{/if}
	{/snippet}
	{#snippet right()}
		{#if snapshot}
			<span class="font-mono tabular-nums">
				{snapshot.sessions.length} conn
			</span>
			<span class="text-fg-muted">·</span>
			<span class="font-mono tabular-nums">
				{snapshot.slowQueries.length} slow
			</span>
		{/if}
	{/snippet}
</StatusBar>
