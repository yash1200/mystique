<script lang="ts">
	/**
	 * Same-engine schema diff. Table renames are only the pairs chosen here;
	 * column renames are paired by hand or guessed by the server, and a guess
	 * can be rejected. Every change has a checkbox; drops start unchecked.
	 * Apply runs on the other connection, through one write-guard decision.
	 */
	import type {
		DiffSelection,
		DropItem,
		RenamePair,
		SchemaChange,
		SchemaMigration,
		TableDefinition,
		TableMeta,
	} from '#core/contracts'
	import { errorMessage, applyEngineSchemaDiff, listEngineTableDefinitions, planEngineSchemaDiff } from '#core/ipc'
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
	import { untrack } from 'svelte'
	import {
		addedColumns,
		emptySelection,
		keepColumnsApart,
		pairColumns,
		pairRename,
		setChangeSelected,
		setDropIncluded,
		sourceTableOf,
		tablePairs,
		unpairColumns,
		unpairedNames,
	} from '../structure-edit'

	interface Props {
		connectionId: string
		engine: string
		table: TableMeta
		onclose: () => void
	}

	let { connectionId, engine, table, onclose }: Props = $props()

	let targetId = $state('')
	let loadedFor = $state('')
	let source = $state<TableDefinition[]>([])
	let target = $state<TableDefinition[]>([])
	let renames = $state<RenamePair[]>([])
	let selection = $state<DiffSelection>(emptySelection())
	let renameFrom = $state('')
	let renameTo = $state('')
	/** Column pair being chosen, per change name. */
	let columnPick = $state<Record<string, { from: string; to: string }>>({})
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
	const unpaired = $derived(
		unpairedNames(
			source.map((item) => item.name),
			target.map((item) => item.name),
			renames,
		),
	)
	const shownPairs = $derived(tablePairs(renames))
	const requestKey = $derived(
		JSON.stringify({
			targetId: activeTargetId,
			renames,
			selection,
			source: source.map((item) => item.name),
			target: target.map((item) => item.name),
		}),
	)
	const fresh = $derived(!!migration && plannedKey === requestKey)
	const blocked = $derived(!migration || migration.warnings.length > 0)
	const changes = $derived(
		(migration?.changes ?? []).filter(
			(change) => change.action !== 'unchanged' || change.drops.length > 0,
		),
	)
	const unchanged = $derived(
		(migration?.changes ?? []).filter(
			(change) => change.action === 'unchanged' && change.drops.length === 0,
		),
	)
	const chosen = $derived(
		changes.filter((change) => change.selected && change.plan.statements.length > 0),
	)
	const canApply = $derived(
		fresh &&
			!blocked &&
			!busy &&
			!!targetSession &&
			!targetSession.profile.readOnly &&
			chosen.length > 0,
	)

	// Load when the chosen connection changes, not on every session update,
	// so rename pairs and checkboxes survive unrelated workspace changes.
	$effect(() => {
		const id = activeTargetId
		if (!id) return
		untrack(() => {
			if (id !== loadedFor) void load(id)
		})
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
		loadedFor = id
		loading = true
		error = null
		migration = null
		plannedKey = ''
		renames = []
		selection = emptySelection()
		columnPick = {}
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
			if (loadedFor !== id) return
			source = nextSource
			target = nextTarget
		} catch (e) {
			error = errorMessage(e)
			source = []
			target = []
		} finally {
			loading = false
		}
	}

	/** After a pair or a checkbox changes, plan again if a preview is showing. */
	function replan() {
		if (migration) void preview()
	}

	function addPair() {
		renames = pairRename(renames, renameFrom, renameTo)
		renameFrom = ''
		renameTo = ''
		replan()
	}

	function removePair(pair: RenamePair) {
		renames = renames.filter((item) => item !== pair)
		replan()
	}

	function toggleChange(change: SchemaChange, on: boolean) {
		selection = setChangeSelected(selection, change, on)
		replan()
	}

	function toggleDrop(change: SchemaChange, drop: DropItem, on: boolean) {
		selection = setDropIncluded(
			selection,
			{ table: change.name, kind: drop.kind, name: drop.name },
			on,
		)
		replan()
	}

	function columnPairsFor(change: SchemaChange): RenamePair[] {
		const table = sourceTableOf(change)
		return renames.filter((pair) => pair.table === table)
	}

	function pickColumns(change: SchemaChange) {
		const pick = columnPick[change.name]
		if (!pick?.from || !pick.to) return
		renames = pairColumns(renames, sourceTableOf(change), pick.from, pick.to)
		columnPick = { ...columnPick, [change.name]: { from: '', to: '' } }
		replan()
	}

	function notARename(change: SchemaChange, from: string, to: string) {
		renames = keepColumnsApart(renames, sourceTableOf(change), from, to)
		replan()
	}

	function forgetColumnPair(change: SchemaChange, pair: RenamePair) {
		renames = unpairColumns(renames, sourceTableOf(change), pair.from, pair.to)
		replan()
	}

	function droppedColumns(change: SchemaChange): string[] {
		return change.drops.filter((drop) => drop.kind === 'column').map((drop) => drop.name)
	}

	function dropLabel(kind: string): string {
		switch (kind) {
			case 'column':
				return 'Drop column'
			case 'index':
				return 'Drop index'
			case 'foreignKey':
				return 'Drop foreign key'
			case 'constraint':
				return 'Drop constraint'
			default:
				return 'Drop'
		}
	}

	async function preview() {
		if (!targetSession || busy) return
		const key = requestKey
		busy = true
		error = null
		let again = false
		try {
			const sourceNow = $state.snapshot(source)
			const targetNow = $state.snapshot(target)
			const renamesNow = $state.snapshot(renames)
			const selectionNow = $state.snapshot(selection)
			const next = await withLiveSession(targetSession.profile.id, (sessionId) =>
				planEngineSchemaDiff({
					sessionId,
					source: sourceNow,
					target: targetNow,
					renames: renamesNow,
					selection: selectionNow,
				}),
			)
			if (requestKey !== key) {
				// Something changed while planning: plan the latest choice.
				again = true
				return
			}
			migration = next
			plannedKey = key
		} catch (e) {
			error = errorMessage(e)
		} finally {
			busy = false
		}
		if (again) void preview()
	}

	async function apply() {
		if (!targetSession || !canApply) return
		const id = targetSession.profile.id
		const sourceNow = $state.snapshot(source)
		const targetNow = $state.snapshot(target)
		const renamesNow = $state.snapshot(renames)
		const selectionNow = $state.snapshot(selection)
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
							selection: selectionNow,
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
			error = errorMessage(e)
		} finally {
			busy = false
		}
	}
</script>

<Modal
	open
	{onclose}
	title="Compare schema"
	description="Update the other connection so its tables match this one. Pair tables or columns yourself when a name changed."
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
				Pair two tables to rename one into the other. Unpaired names are created, or listed
				as drops you can choose.
			</p>
			<div class="flex flex-wrap items-center gap-1.5">
				<Select
					bind:value={renameFrom}
					options={unpaired.sourceOnly.map((name) => ({ value: name, label: name }))}
					placeholder="Table here"
					ariaLabel="Table on this connection"
					inputSize="sm"
					class="w-40"
				/>
				<Select
					bind:value={renameTo}
					options={unpaired.targetOnly.map((name) => ({ value: name, label: name }))}
					placeholder="Table there"
					ariaLabel="Table on the other connection"
					inputSize="sm"
					class="w-40"
				/>
				<Button variant="secondary" size="sm" onclick={addPair} disabled={!renameFrom || !renameTo}>
					Pair as rename
				</Button>
			</div>
			{#if shownPairs.length > 0}
				<ul class="m-0 list-none space-y-1 p-0">
					{#each shownPairs as pair (pair.from + pair.to)}
						<li class="flex items-center gap-2 font-mono text-label text-fg">
							<span>{pair.from} → {pair.to}</span>
							<Button variant="ghost" size="sm" onclick={() => removePair(pair)}>
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
		{#if migration}
			{#each changes as change (`${change.action}:${change.name}`)}
				{@const added = addedColumns(change)}
				{@const dropped = droppedColumns(change)}
				{@const pairs = columnPairsFor(change)}
				{@const pick = columnPick[change.name] ?? { from: '', to: '' }}
				<article
					class="rounded-md border border-line bg-panel px-2.5 py-2"
					class:opacity-60={!change.selected}
				>
					<label class="m-0 flex items-center gap-2 font-mono text-label text-fg">
						<input
							type="checkbox"
							checked={change.selected}
							disabled={busy}
							onchange={(e) => toggleChange(change, e.currentTarget.checked)}
						/>
						<span class="text-fg-muted">{change.action}</span>{change.name}
					</label>
					{#if change.action === 'drop' && !change.selected}
						<p class="mt-1 mb-0 text-caption text-fg-muted">
							Only on the other connection. Check it to drop the table.
						</p>
					{/if}
					{#if change.plan.note}
						<p class="mt-1 mb-0 text-caption text-fg-muted">{change.plan.note}</p>
					{/if}
					{#if change.columnRenames.length > 0 || pairs.some((pair) => pair.keepApart)}
						<ul class="m-0 mt-1 list-none space-y-1 p-0">
							{#each change.columnRenames as rename (rename.from + rename.to)}
								<li class="flex flex-wrap items-center gap-2 text-caption text-fg-secondary">
									<span class="font-mono text-fg">{rename.to} → {rename.from}</span>
									<span>{rename.guessed ? 'Guessed rename' : 'Rename you paired'}</span>
									{#if rename.guessed}
										<Button
											variant="ghost"
											size="sm"
											disabled={busy}
											onclick={() => notARename(change, rename.from, rename.to)}
										>
											Not a rename
										</Button>
									{:else}
										<Button
											variant="ghost"
											size="sm"
											disabled={busy}
											onclick={() => forgetColumnPair(change, { from: rename.from, to: rename.to })}
										>
											Unpair
										</Button>
									{/if}
								</li>
							{/each}
							{#each pairs.filter((pair) => pair.keepApart) as pair (pair.from + pair.to)}
								<li class="flex flex-wrap items-center gap-2 text-caption text-fg-secondary">
									<span class="font-mono text-fg">{pair.to} ≠ {pair.from}</span>
									<span>Not a rename</span>
									<Button
										variant="ghost"
										size="sm"
										disabled={busy}
										onclick={() => forgetColumnPair(change, pair)}
									>
										Undo
									</Button>
								</li>
							{/each}
						</ul>
					{/if}
					{#if added.length > 0 && dropped.length > 0}
						<div class="mt-1.5 flex flex-wrap items-center gap-1.5">
							<Select
								value={pick.to}
								onchange={(value) =>
									(columnPick = { ...columnPick, [change.name]: { ...pick, to: value } })}
								options={dropped.map((name) => ({ value: name, label: name }))}
								placeholder="Column there"
								ariaLabel="Column on the other connection"
								inputSize="sm"
								class="w-36"
							/>
							<Select
								value={pick.from}
								onchange={(value) =>
									(columnPick = { ...columnPick, [change.name]: { ...pick, from: value } })}
								options={added.map((name) => ({ value: name, label: name }))}
								placeholder="Column here"
								ariaLabel="Column on this connection"
								inputSize="sm"
								class="w-36"
							/>
							<Button
								variant="secondary"
								size="sm"
								disabled={busy || !pick.from || !pick.to}
								onclick={() => pickColumns(change)}
							>
								Pair columns
							</Button>
						</div>
					{/if}
					{#if change.action !== 'drop' && change.drops.length > 0}
						<fieldset class="m-0 mt-1.5 border-0 p-0">
							<legend class="text-caption text-fg-muted">
								Only on the other connection. Checked items are dropped.
							</legend>
							{#each change.drops as drop (drop.kind + ':' + drop.name)}
								<label class="flex items-center gap-2 text-caption text-fg-secondary">
									<input
										type="checkbox"
										checked={drop.included}
										disabled={busy}
										onchange={(e) => toggleDrop(change, drop, e.currentTarget.checked)}
									/>
									<span>{dropLabel(drop.kind)}</span>
									<span class="font-mono text-fg">{drop.name}</span>
								</label>
							{/each}
						</fieldset>
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
				: 'Apply the checked changes'}
		>
			Apply
		</Button>
	{/snippet}
</Modal>
