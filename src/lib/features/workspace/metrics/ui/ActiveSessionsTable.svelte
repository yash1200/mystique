<script lang="ts">
	import type { ActiveBackendSession } from '#core/contracts'
	import { Button, EmptyState, Pill } from '#core/ui'
	import { formatDurationMs } from '../format'
	import { filterSessions, sessionStateTone } from '../presentation'

	interface Props {
		sessions: ActiveBackendSession[]
		filter: string
		supported: boolean
		canTerminate: boolean
		onend: (row: ActiveBackendSession) => void
	}

	let { sessions, filter, supported, canTerminate, onend }: Props = $props()

	const rows = $derived(filterSessions(sessions, filter))
</script>

{#if !supported}
	<EmptyState
		title="No session list"
		description="This engine does not expose live backend sessions."
	/>
{:else if rows.length === 0}
	<EmptyState
		title={filter.trim() ? 'No matching sessions' : 'No connections'}
		description={filter.trim()
			? 'Try a different filter.'
			: 'The server did not report any sessions.'}
	/>
{:else}
	<div class="overflow-x-auto">
		<table class="w-full min-w-160 border-collapse text-left">
			<thead>
				<tr class="text-caption font-medium text-fg-muted">
					<th class="px-2 py-1.5 font-medium">Pid</th>
					<th class="px-2 py-1.5 font-medium">User</th>
					<th class="px-2 py-1.5 font-medium">Database</th>
					<th class="px-2 py-1.5 font-medium">State</th>
					<th class="px-2 py-1.5 font-medium">Duration</th>
					<th class="px-2 py-1.5 font-medium">Query</th>
					{#if canTerminate}
						<th class="w-20 px-2 py-1.5 font-medium"> </th>
					{/if}
				</tr>
			</thead>
			<tbody>
				{#each rows as row (row.id)}
					<tr class="border-t border-line hover:bg-hover">
						<td class="px-2 py-1.5 font-mono text-caption tabular-nums text-fg">
							{row.id}
						</td>
						<td class="px-2 py-1.5 text-caption text-fg">
							{row.user ?? '—'}
						</td>
						<td class="px-2 py-1.5 font-mono text-caption text-fg-secondary">
							{row.database ?? '—'}
						</td>
						<td class="px-2 py-1.5">
							<Pill tone={sessionStateTone(row.state)}>{row.state ?? '—'}</Pill>
						</td>
						<td
							class="px-2 py-1.5 font-mono text-caption tabular-nums text-fg-secondary"
						>
							{row.durationMs != null ? formatDurationMs(row.durationMs) : '—'}
						</td>
						<td
							class="max-w-xs truncate px-2 py-1.5 font-mono text-caption text-fg"
							title={row.query ?? ''}
						>
							{row.query || '—'}
						</td>
						{#if canTerminate}
							<td class="px-2 py-1.5 text-right">
								{#if row.canTerminate}
									<Button
										variant="danger-ghost"
										size="sm"
										onclick={() => onend(row)}
									>
										End
									</Button>
								{/if}
							</td>
						{/if}
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
{/if}
