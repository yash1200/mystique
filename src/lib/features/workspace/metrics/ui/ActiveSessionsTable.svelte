<script lang="ts">
	import type { ActiveBackendSession } from '#core/contracts'
	import { Button, EmptyHint, Pill } from '#core/ui'
	import { formatDurationMs } from '../format'
	import { filterSessions, sessionStateTone } from '../presentation'

	interface Props {
		sessions: ActiveBackendSession[]
		filter: string
		supported: boolean
		canTerminate: boolean
		onend: (row: ActiveBackendSession) => void
	}

	let {
		sessions,
		filter = $bindable(''),
		supported,
		canTerminate,
		onend,
	}: Props = $props()

	const rows = $derived(filterSessions(sessions, filter))
	const th = 'text-[0.71875rem] font-medium whitespace-nowrap text-fg-muted'
	const mono = 'font-mono text-label whitespace-nowrap tabular-nums'
</script>

{#if !supported}
	<EmptyHint fill icon="plug" title="No session list" description="This engine does not expose live backend sessions." />
{:else if sessions.length === 0}
	<EmptyHint fill icon="plug" title="No connections" description="The server did not report any sessions." />
{:else if rows.length === 0}
	<EmptyHint fill icon="search" title="No matches" description="No connection matches “{filter.trim()}”">
		<Button variant="ghost" size="sm" onclick={() => (filter = '')}>Clear filter</Button>
	</EmptyHint>
{:else}
	<div class="min-h-0 flex-1 overflow-auto">
		<table class="w-full min-w-160 border-collapse text-left">
			<thead class="sticky top-0 z-10 bg-surface">
				<tr class="h-8.5 shadow-[inset_0_-1px_0_var(--line)]">
					<th class="{th} pr-2 pl-4" scope="col">Id</th>
					<th class="{th} px-2" scope="col">User</th>
					<th class="{th} px-2" scope="col">Database</th>
					<th class="{th} px-2" scope="col">State</th>
					<th class="{th} px-2 text-right" scope="col">Duration</th>
					<th class="{th} px-2 w-full" scope="col">Query</th>
					{#if canTerminate}
						<th class="w-16 pr-3" scope="col"><span class="sr-only">Actions</span></th>
					{/if}
				</tr>
			</thead>
			<tbody>
				{#each rows as row (row.id)}
					<tr class="group/row h-10 border-b border-line text-body transition-colors duration-150 hover:bg-surface">
						<td class="{mono} pr-2 pl-4 text-fg">{row.id}</td>
						<td class="px-2 whitespace-nowrap text-fg">{row.user ?? '—'}</td>
						<td class="{mono} px-2 text-fg-secondary">{row.database ?? '—'}</td>
						<td class="px-2">
							{#if row.state}
								<Pill compact tone={sessionStateTone(row.state)}>{row.state}</Pill>
							{:else}
								<span class="text-fg-muted">—</span>
							{/if}
						</td>
						<td class="{mono} px-2 text-right text-fg-secondary">
							{row.durationMs != null ? formatDurationMs(row.durationMs) : '—'}
						</td>
						<td class="max-w-0 px-2">
							<span class="block truncate font-mono text-label text-fg-secondary" title={row.query ?? ''}>
								{row.query || '—'}
							</span>
						</td>
						{#if canTerminate}
							<td class="pr-3 text-right">
								{#if row.canTerminate}
									<div class="opacity-0 transition-opacity group-hover/row:opacity-100 focus-within:opacity-100">
										<Button variant="danger-ghost" size="sm" onclick={() => onend(row)}>End</Button>
									</div>
								{/if}
							</td>
						{/if}
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
{/if}
