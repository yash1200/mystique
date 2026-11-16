<script lang="ts">
	import type { SlowQuery } from '#core/contracts'
	import { Button, EmptyHint, Pill } from '#core/ui'
	import { formatDurationMs } from '../format'
	import { filterSlowQueries } from '../presentation'

	interface Props {
		queries: SlowQuery[]
		filter: string
		supported: boolean
		minDurationMs: number
	}

	let { queries, filter = $bindable(''), supported, minDurationMs }: Props = $props()

	const rows = $derived(filterSlowQueries(queries, filter))
	const th = 'text-[0.71875rem] font-medium whitespace-nowrap text-fg-muted'
	const mono = 'font-mono text-label whitespace-nowrap tabular-nums'
</script>

{#if !supported && queries.length === 0}
	<EmptyHint fill icon="history" title="No slow-query source" description="This engine does not expose a slow-query log." />
{:else if queries.length === 0}
	<EmptyHint
		fill
		icon="history"
		title="Nothing slow right now"
		description="No statements longer than {formatDurationMs(minDurationMs)}."
	/>
{:else if rows.length === 0}
	<EmptyHint fill icon="search" title="No matches" description="No query matches “{filter.trim()}”">
		<Button variant="ghost" size="sm" onclick={() => (filter = '')}>Clear filter</Button>
	</EmptyHint>
{:else}
	<div class="min-h-0 flex-1 overflow-auto">
		<table class="w-full min-w-160 border-collapse text-left">
			<thead class="sticky top-0 z-10 bg-surface">
				<tr class="h-8.5 shadow-[inset_0_-1px_0_var(--line)]">
					<th class="{th} w-full pr-2 pl-4" scope="col">Query</th>
					<th class="{th} px-2 text-right" scope="col">Duration</th>
					<th class="{th} px-2 text-right" scope="col">Mean</th>
					<th class="{th} px-2 text-right" scope="col">Calls</th>
					<th class="{th} px-2" scope="col">Database</th>
					<th class="{th} pr-4 pl-2" scope="col">Source</th>
				</tr>
			</thead>
			<tbody>
				{#each rows as row (row.id)}
					<tr class="h-10 border-b border-line text-body transition-colors duration-150 hover:bg-surface">
						<td class="max-w-0 pr-2 pl-4">
							<span class="block truncate font-mono text-label text-fg" title={row.query}>
								{row.query || '—'}
							</span>
						</td>
						<td class="{mono} px-2 text-right text-fg">
							{row.durationMs != null ? formatDurationMs(row.durationMs) : '—'}
						</td>
						<td class="{mono} px-2 text-right text-fg-secondary">
							{row.meanMs != null ? formatDurationMs(row.meanMs) : '—'}
						</td>
						<td class="{mono} px-2 text-right text-fg-secondary">
							{row.calls != null ? row.calls.toLocaleString() : '—'}
						</td>
						<td class="{mono} px-2 text-fg-secondary">{row.database ?? '—'}</td>
						<td class="pr-4 pl-2">
							{#if row.source}
								<Pill compact tone={row.source === 'live' ? 'info' : 'neutral'}>{row.source}</Pill>
							{:else}
								<span class="text-fg-muted">—</span>
							{/if}
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
{/if}
