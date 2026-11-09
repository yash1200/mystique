<script lang="ts">
	import { Button, Input, Select, Switch } from '#core/ui'
	import { engineLabel } from '#engines'
	import { catalogStore } from '#features/workspace/catalog'
	import {
		INHERIT,
		inheritedLabel,
		isAccessLevel,
		levelLabel,
		levelOptions,
	} from '../access-levels'
	import type { AccessLevel, ToolAccessConnection } from '../mcp-types'

	interface Props {
		connections: ToolAccessConnection[]
		busy: boolean
		onlevel: (
			connectionId: string,
			database: string | null,
			level: AccessLevel | null,
		) => void
		onask: (connectionId: string, ask: boolean) => void
	}

	let { connections, busy, onlevel, onask }: Props = $props()

	/** Connection id → the database name being added. */
	let drafts = $state<Record<string, string>>({})
	/** Connection ids with the database list open. */
	let open = $state<Record<string, boolean>>({})

	function pick(connectionId: string, database: string | null, value: string) {
		onlevel(connectionId, database, isAccessLevel(value) ? value : null)
	}

	/** Catalog names when the connection is open; otherwise free text. */
	function knownDatabases(c: ToolAccessConnection): string[] {
		const set = new Set(c.databases.map((d) => d.name))
		return catalogStore
			.databases(c.id)
			.map((d) => d.name)
			.filter((name) => !set.has(name))
	}

	function addDatabase(c: ToolAccessConnection) {
		const name = (drafts[c.id] ?? '').trim()
		if (!name) return
		// A new database row starts at what it inherits, made explicit.
		const start = c.level ?? c.mcpLevel
		onlevel(c.id, name, isAccessLevel(start) ? start : 'read')
		drafts = { ...drafts, [c.id]: '' }
	}

	function connectionInherit(c: ToolAccessConnection): string {
		return inheritedLabel(c.mcpLevel, c.agentLevel)
	}

	function databaseInherit(c: ToolAccessConnection): string {
		return c.level ? levelLabel(c.level) : connectionInherit(c)
	}
</script>

<div class="overflow-hidden rounded-lg border border-line bg-panel">
	<ul class="m-0 flex list-none flex-col divide-y divide-line p-0">
		{#each connections as c (c.id)}
			{@const known = knownDatabases(c)}
			<li class="flex flex-col gap-2 px-3 py-2.5">
				<div class="flex items-center gap-3">
					<span class="flex min-w-0 flex-1 flex-col">
						<span class="truncate text-label font-medium text-fg"
							>{c.name}</span
						>
						<span class="text-caption text-fg-muted">
							{engineLabel(c.engine)}
							{#if c.environment === 'production'}
								· production
							{/if}
							{#if c.level == null}
								· inherits {connectionInherit(c)}
							{/if}
						</span>
					</span>
					<Select
						inputSize="sm"
						variant="quiet"
						ariaLabel={`Access for ${c.name}`}
						value={c.level ?? INHERIT}
						options={levelOptions(connectionInherit(c))}
						disabled={busy}
						onchange={(v) => pick(c.id, null, v)}
					/>
				</div>
				<Switch
					checked={c.askHighRisk}
					disabled={busy}
					label="Ask me for high-risk writes"
					description="A write above the level waits in the approval dock for 90 seconds instead of being refused."
					onchange={(on) => onask(c.id, on)}
				/>
				<div>
					<Button
						size="sm"
						variant="ghost"
						onclick={() => (open = { ...open, [c.id]: !open[c.id] })}
					>
						{open[c.id] ? 'Hide databases' : 'Databases'}
						{#if c.databases.length > 0}
							({c.databases.length})
						{/if}
					</Button>
				</div>
				{#if open[c.id]}
					<div class="flex flex-col gap-1.5 pl-2">
						{#each c.databases as d (d.name)}
							<div class="flex items-center gap-3">
								<span class="min-w-0 flex-1 truncate font-mono text-caption text-fg"
									>{d.name}</span
								>
								<Select
									inputSize="sm"
									variant="quiet"
									ariaLabel={`Access for ${d.name} on ${c.name}`}
									value={d.level}
									options={levelOptions(databaseInherit(c))}
									disabled={busy}
									onchange={(v) => pick(c.id, d.name, v)}
								/>
							</div>
						{/each}
						<div class="flex items-center gap-1.5">
							{#if known.length > 0}
								<Select
									inputSize="sm"
									ariaLabel={`Database on ${c.name}`}
									placeholder="Pick a database"
									value={drafts[c.id] ?? ''}
									options={known.map((name) => ({ value: name, label: name }))}
									disabled={busy}
									onchange={(v) => (drafts = { ...drafts, [c.id]: v })}
								/>
							{:else}
								<Input
									inputSize="sm"
									aria-label={`Database on ${c.name}`}
									placeholder={c.defaultDatabase ?? 'Database name'}
									value={drafts[c.id] ?? ''}
									disabled={busy}
									oninput={(e) =>
										(drafts = {
											...drafts,
											[c.id]: (e.currentTarget as HTMLInputElement).value,
										})}
									onkeydown={(e) => {
										if (e.key === 'Enter') addDatabase(c)
									}}
								/>
							{/if}
							<Button
								size="sm"
								disabled={busy || !(drafts[c.id] ?? '').trim()}
								onclick={() => addDatabase(c)}
							>
								Set level
							</Button>
						</div>
						<p class="m-0 text-caption text-fg-muted">
							A database's level wins over the connection's. Choose Inherit to
							remove it.
						</p>
					</div>
				{/if}
			</li>
		{/each}
	</ul>
</div>
