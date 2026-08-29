<script lang="ts">
	import type { SlowQuery } from '$core/contracts'
	import { EmptyState, Pill } from '$core/ui'
	import { formatDurationMs } from '../format'
	import { filterSlowQueries } from '../presentation'

	interface Props {
		queries: SlowQuery[]
		filter: string
		supported: boolean
		minDurationMs: number
	}

	let { queries, filter, supported, minDurationMs }: Props = $props()

	const rows = $derived(filterSlowQueries(queries, filter))
</script>

{#if !supported && rows.length === 0}
	<EmptyState
		title="No slow-query source"
		description="This engine does not expose a slow-query log."
	/>
{:else if rows.length === 0}
	<EmptyState
		title={filter.trim() ? 'No matching queries' : 'Nothing slow right now'}
		description={filter.trim()
			? 'Try a different filter.'
			: `No statements longer than ${formatDurationMs(minDurationMs)}.`}
	/>
{:else}
	<div class="overflow-x-auto">
		<table class="w-full min-w-160 border-collapse text-left">
			<thead>
				<tr class="text-caption font-medium text-fg-muted">
					<th class="px-2 py-1.5 font-medium">Query</th>
					<th class="px-2 py-1.5 font-medium">Duration</th>
					<th class="px-2 py-1.5 font-medium">Mean</th>
					<th class="px-2 py-1.5 font-medium">Calls</th>
					<th class="px-2 py-1.5 font-medium">Database</th>
					<th class="px-2 py-1.5 font-medium">Source</th>
				</tr>
			</thead>
			<tbody>
				{#each rows as row (row.id)}
					<tr class="border-t border-line hover:bg-hover">
						<td
							class="max-w-md truncate px-2 py-1.5 font-mono text-caption text-fg"
							title={row.query}
						>
							{row.query || '—'}
						</td>
						<td class="px-2 py-1.5 font-mono text-caption tabular-nums text-fg">
							{row.durationMs != null ? formatDurationMs(row.durationMs) : '—'}
						</td>
						<td
							class="px-2 py-1.5 font-mono text-caption tabular-nums text-fg-secondary"
						>
							{row.meanMs != null ? formatDurationMs(row.meanMs) : '—'}
						</td>
						<td
							class="px-2 py-1.5 font-mono text-caption tabular-nums text-fg-secondary"
						>
							{row.calls ?? '—'}
						</td>
						<td class="px-2 py-1.5 font-mono text-caption text-fg-secondary">
							{row.database ?? '—'}
						</td>
						<td class="px-2 py-1.5">
							<Pill tone={row.source === 'live' ? 'info' : 'neutral'}>
								{row.source ?? '—'}
							</Pill>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
{/if}
