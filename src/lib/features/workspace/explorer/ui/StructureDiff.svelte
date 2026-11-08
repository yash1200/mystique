<script lang="ts">
	/**
	 * Same-engine schema diff. Renames are only the pairs chosen here.
	 * Apply runs on the other connection, through one write-guard decision.
	 */
	import type { RenamePair, SchemaMigration, TableDefinition, TableMeta } from '#core/contracts'
	import { applyEngineSchemaDiff, listEngineTableDefinitions, planEngineSchemaDiff } from '#core/ipc'
	import { Alert, Button, Modal, Select } from '#core/ui'
	import { runtimeFor } from '#engines'
	import { showToast } from '#features/toast'
	import { ensureLiveCatalog } from '#features/workspace/catalog'
	import {
		isWriteCancelled,
		runWithWriteGuard,
		sessionDatabase,
		withLiveSession,
		workspaceSession,
	} from '#features/workspace/session'
	import { pairRename, unpairedNames } from '../structure-edit'

	interface Props {
		connectionId: string
		engine: string
		table: TableMeta
		onclose: () => void
	}

	let { connectionId, engine, table, onclose }: Props = $props()

	let targetId = $state('')
	let source = $state<TableDefinition[]>([])
	let target = $state<TableDefinition[]>([])
	let renames = $state<RenamePair[]>([])
	let renameFrom = $state('')
	let renameTo = $state('')
	let migration = $state<SchemaMigration | null>(null)
	let plannedKey = $state('')
	let loading = $state(false)
	let busy = $state(false)
	let error = $state<string | null>(null)

	const peers = $derived(
		workspaceSession.sessions.filter((session) => {
			if (session.profile.id === connectionId) return false
			const runtime = runtimeFor(engine)
			return !!runtime && runtimeFor(String(session.profile.engine)) === runtime
		}),
	)
	const activeTargetId = $derived(targetId || peers[0]?.profile.id || '')
	const targetSession = $derived(
		peers.find((session) => session.profile.id === activeTargetId) ?? null,
	)
	const sourceOnly = $derived(
		unpairedNames(
			source.map((item) => item.name),
			target.map((item) => item.name),
			renames,
		).sourceOnly,
	)
	const targetOnly = $derived(
		unpairedNames(
			source.map((item) => item.name),
			target.map((item) => item.name),
			renames,
		).targetOnly,
	)
	const requestKey = $derived(
		JSON.stringify({
			targetId: activeTargetId,
			renames,
			source: source.map((item) => item.name),
			target: target.map((item) => item.name),
		}),
	)
	const fresh = $derived(!!migration && plannedKey === requestKey)
	const blocked = $derived(!migration || migration.warnings.length > 0)
	const changes = $derived(
		(migration?.changes ?? []).filter((change) => change.action !== 'unchanged'),
	)
	const unchanged = $derived(
		(migration?.changes ?? []).filter((change) => change.action === 'unchanged'),
	)
	const canApply = $derived(
		fresh &&
			!blocked &&
			!busy &&
			!!targetSession &&
			!targetSession.profile.readOnly &&
			changes.length > 0,
	)

	$effect(() => {
		const id = activeTargetId
		if (!id) return
		void load(id)
	})

	function scope(database: string | null, schema: string | null) {
		return {
			database: database || null,
			schema: schema || null,
		}
	}

	async function load(id: string) {
		const session = peers.find((item) => item.profile.id === id)
		if (!session) return
		loading = true
		error = null
		migration = null
		plannedKey = ''
		renames = []
		try {
			const sourceScope = scope(table.database || null, table.schema || null)
			const targetScope = scope(
				sessionDatabase(id, session.profile) || table.database || null,
				table.schema || null,
			)
			const [nextSource, nextTarget] = await Promise.all([
				withLiveSession(connectionId, (sessionId) =>
					listEngineTableDefinitions({ sessionId, ...sourceScope }),
				),
				withLiveSession(id, (sessionId) =>
					listEngineTableDefinitions({ sessionId, ...targetScope }),
				),
			])
			if (activeTargetId !== id) return
			source = nextSource
			target = nextTarget
		} catch (e) {
			error = e instanceof Error ? e.message : String(e)
			source = []
			target = []
		} finally {
			loading = false
		}
	}

	function addPair() {
		renames = pairRename(renames, renameFrom, renameTo)
		renameFrom = ''
		renameTo = ''
	}

	async function preview() {
		if (!targetSession || busy) return
		const key = requestKey
		busy = true
		error = null
		try {
			const next = await withLiveSession(targetSession.profile.id, (sessionId) =>
				planEngineSchemaDiff({
					sessionId,
					source,
					target,
					renames,
				}),
			)
			if (requestKey !== key) return
			migration = next
			plannedKey = key
		} catch (e) {
			error = e instanceof Error ? e.message : String(e)
		} finally {
			busy = false
		}
	}

	async function apply() {
		if (!targetSession || !canApply) return
		const id = targetSession.profile.id
		const readOnly = targetSession.profile.readOnly
		const sourceNow = $state.snapshot(source)
		const targetNow = $state.snapshot(target)
		const renamesNow = $state.snapshot(renames)
		busy = true
		error = null
		try {
			let appliedSession = ''
			const result = await withLiveSession(
				id,
				(sessionId) => {
					appliedSession = sessionId
					return runWithWriteGuard(sessionId, (confirmObjectName) =>
						applyEngineSchemaDiff({
							sessionId,
							source: sourceNow,
							target: targetNow,
							renames: renamesNow,
							profileReadOnly: readOnly,
							confirmObjectName: confirmObjectName ?? null,
						}),
					)
				},
				{ replay: false },
			)
			showToast('success', result.message || 'Schema updated.')
			await ensureLiveCatalog(
				id,
				String(targetSession.profile.engine),
				appliedSession || targetSession.engineSessionId || id,
				sessionDatabase(id, targetSession.profile),
				true,
			)
			onclose()
		} catch (e) {
			if (isWriteCancelled(e)) return
			error = e instanceof Error ? e.message : String(e)
		} finally {
			busy = false
		}
	}
</script>

<Modal
	open
	{onclose}
	title="Compare schema"
	description="Update the other connection so its tables match this one. Pair tables yourself when a name changed."
	size="xl"
	layout="chrome"
	height="lg"
>
	<div class="min-h-0 flex-1 space-y-3 overflow-auto px-4 py-3">
		{#if peers.length === 0}
			<p class="m-0 text-caption text-fg-muted">
				Open another connection of this engine to compare.
			</p>
		{:else}
			<Select
				value={activeTargetId}
				onchange={(value) => (targetId = value)}
				options={peers.map((session) => ({
					value: session.profile.id,
					label: session.profile.name,
				}))}
				ariaLabel="Connection to update"
				inputSize="sm"
			/>
			<p class="m-0 text-caption text-fg-muted">
				Pair two tables to rename one into the other. Unpaired names are created or dropped.
			</p>
			<div class="flex flex-wrap items-center gap-1.5">
				<Select
					bind:value={renameFrom}
					options={sourceOnly.map((name) => ({ value: name, label: name }))}
					placeholder="Table here"
					ariaLabel="Table on this connection"
					inputSize="sm"
					class="w-40"
				/>
				<Select
					bind:value={renameTo}
					options={targetOnly.map((name) => ({ value: name, label: name }))}
					placeholder="Table there"
					ariaLabel="Table on the other connection"
					inputSize="sm"
					class="w-40"
				/>
				<Button variant="secondary" size="sm" onclick={addPair} disabled={!renameFrom || !renameTo}>
					Pair as rename
				</Button>
			</div>
			{#if renames.length > 0}
				<ul class="m-0 list-none space-y-1 p-0">
					{#each renames as pair (pair.from + pair.to)}
						<li class="flex items-center gap-2 font-mono text-label text-fg">
							<span>{pair.from} → {pair.to}</span>
							<Button
								variant="ghost"
								size="sm"
								onclick={() => (renames = renames.filter((item) => item !== pair))}
							>
								Remove
							</Button>
						</li>
					{/each}
				</ul>
			{/if}
		{/if}

		{#if error}
			<Alert tone="danger">{error}</Alert>
		{/if}
		{#if loading}
			<p class="m-0 text-caption text-fg-muted">Loading tables…</p>
		{/if}
		{#if migration && fresh && migration.warnings.length > 0}
			{#each migration.warnings as warning, i (i)}
				<Alert tone="warning">{warning}</Alert>
			{/each}
		{/if}
		{#if fresh}
			{#each changes as change (`${change.action}:${change.name}`)}
				<article class="rounded-md border border-line bg-panel px-2.5 py-2">
					<h3 class="m-0 font-mono text-label text-fg">
						<span class="mr-1.5 text-fg-muted">{change.action}</span>{change.name}
					</h3>
					{#if change.plan.note}
						<p class="mt-1 mb-0 text-caption text-fg-muted">{change.plan.note}</p>
					{/if}
					{#if change.plan.statements.length > 0}
						<pre class="mt-1 mb-0 overflow-auto font-mono text-caption text-fg whitespace-pre-wrap">{change.plan.statements.join('\n')}</pre>
					{/if}
				</article>
			{/each}
			{#if unchanged.length > 0}
				<details>
					<summary class="cursor-pointer text-caption text-fg-muted">
						Unchanged ({unchanged.length})
					</summary>
					<ul class="m-0 mt-1 list-none p-0 font-mono text-caption text-fg-secondary">
						{#each unchanged as change (change.name)}
							<li>{change.name}</li>
						{/each}
					</ul>
				</details>
			{/if}
		{/if}
	</div>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={onclose}>Cancel</Button>
		<Button
			variant="secondary"
			size="sm"
			onclick={() => void preview()}
			disabled={busy || !targetSession || loading}
		>
			Preview
		</Button>
		<Button
			variant="primary"
			size="sm"
			onclick={() => void apply()}
			disabled={!canApply}
			title={targetSession?.profile.readOnly
				? 'This connection is read-only.'
				: 'Apply the migration'}
		>
			Apply
		</Button>
	{/snippet}
</Modal>
