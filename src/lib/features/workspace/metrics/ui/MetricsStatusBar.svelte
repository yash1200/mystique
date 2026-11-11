<script lang="ts">
	import type { ServerMetricsSnapshot } from '#core/contracts'
	import { FetchedAgo, StatusBar } from '#core/ui'

	interface Props {
		fetchedAt: number | null
		live: boolean
		snapshot: ServerMetricsSnapshot | null
	}

	let { fetchedAt, live, snapshot }: Props = $props()
</script>

<StatusBar ariaLabel="Metrics status">
	{#snippet left()}
		{#if fetchedAt}
			<FetchedAgo at={fetchedAt} prefix="Collected" />
		{:else}
			<span>Loading…</span>
		{/if}
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
