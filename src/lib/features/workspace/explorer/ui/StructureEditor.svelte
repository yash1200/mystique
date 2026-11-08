<script lang="ts">
	/**
	 * Edit a table definition. The server plans the SQL; this view only edits the draft.
	 * The SQL panel replans as you edit; Apply sends the draft and the server replans it.
	 */
	import type { TableDefinition, TableMeta } from '#core/contracts'
	import { errorMessage, applyEngineDdl, describeEngineTableDefinition, planEngineDdl } from '#core/ipc'
	import { Alert, Button, EmptyState } from '#core/ui'
	import { latestRun } from '#core/util'
	import { showToast } from '#features/toast'
	import { refreshCurrentTables } from '#features/workspace/catalog'
	import {
		currentDatabaseName,
		isWriteCancelled,
		runWithWriteGuard,
		withLiveSession,
	} from '#features/workspace/session'
	import { onMount } from 'svelte'
	import { openObjectStructure } from '#features/workspace/structure'
	import {
		addColumn,
		addForeignKey,
		addIndex,
		columnListText,
		draftFingerprint,
		draftToSend,
		dropColumn,
		dropForeignKey,
		dropIndex,
		isCollection,
		restoreColumn,
		restoreForeignKey,
		restoreIndex,
		setForeignKeyColumns,
		setIndexColumns,
	} from '../structure-edit'

	interface Props {
		connectionId: string
		table: TableMeta
		readOnly?: boolean
		showForeignKeys?: boolean
		/** Engine can only add columns, rename, and change indexes. Other column edits are locked. */
		limited?: boolean
		ondone?: () => void
	}

	let {
		connectionId,
		table,
		readOnly = false,
		showForeignKeys = false,
		limited = false,
		ondone,
	}: Props = $props()

	const field =
		'h-control-sm w-full min-w-0 rounded-md border border-line bg-elevated px-2 font-mono text-label text-fg outline-none focus:border-line-strong'

	let draft = $state<TableDefinition | null>(null)
	let loading = $state(true)
	let busy = $state(false)
	let error = $state<string | null>(null)
	let plan = $state<Awaited<ReturnType<typeof planEngineDdl>> | null>(null)
	let planError = $state<string | null>(null)
	let validatorText = $state('')

	const collection = $derived(draft ? isCollection(draft) : false)
	const canApply = $derived(!readOnly && !busy && !!draft)

	onMount(() => {
		void load()
	})

	async function load() {
		loading = true
		error = null
		try {
			const loaded = await withLiveSession(connectionId, (sessionId) =>
				describeEngineTableDefinition({
					sessionId,
					table: table.name,
					schema: table.schema || null,
					database: table.database || currentDatabaseName() || null,
					kind: table.kind,
				}),
			)
			draft = loaded
			validatorText = loaded.validator ?? ''
			plan = null
		} catch (e) {
			error = errorMessage(e)
		} finally {
			loading = false
		}
	}

	const autoPlan = latestRun({
		delayMs: 300,
		run: () => {
			const next = draft ? draftToSend(draft, validatorText) : null
			if (!next) return Promise.resolve(null)
			return withLiveSession(connectionId, (sessionId) =>
				planEngineDdl({ sessionId, draft: next }),
			)
		},
		onResult: (next) => {
			plan = next
			planError = null
		},
		onError: (e) => {
			planError = errorMessage(e)
		},
	})

	$effect(() => {
		if (!draft) return
		// Replan whenever the draft or the validator text changes.
		void draftFingerprint(draft)
		void validatorText
		autoPlan.schedule()
		return () => autoPlan.cancel()
	})

	async function apply() {
		if (!draft || !canApply) return
		const snapshot = draftToSend(draft, validatorText)
		busy = true
		error = null
		try {
			const result = await withLiveSession(
				connectionId,
				(sessionId) =>
					runWithWriteGuard(sessionId, (confirmObjectName) =>
						applyEngineDdl({
							sessionId,
							draft: snapshot,
							confirmObjectName: confirmObjectName ?? null,
						}),
					),
				{ replay: false },
			)
			showToast('success', result.message || 'Structure updated.')
			await openObjectStructure(table, true)
			await refreshCurrentTables()
			ondone?.()
		} catch (e) {
			if (isWriteCancelled(e)) return
			error = errorMessage(e)
		} finally {
			busy = false
		}
	}
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash">
	<header
		class="flex shrink-0 items-center gap-2 border-b border-line bg-elevated px-2.5 py-2"
	>
		<h1 class="m-0 min-w-0 flex-1 truncate font-mono text-label font-semibold text-fg">
			Edit {table.name}
		</h1>
		<Button variant="ghost" size="sm" onclick={() => ondone?.()} disabled={busy}>
			Cancel
		</Button>
		<Button
			variant="primary"
			size="sm"
			onclick={() => void apply()}
			disabled={!canApply}
			title={readOnly ? 'This connection is read-only.' : 'Apply this change'}
		>
			Apply
		</Button>
	</header>

	{#if loading}
		<div class="flex flex-1 items-center justify-center p-8">
			<EmptyState title="Loading definition…" description="Reading columns, indexes, and keys." />
		</div>
	{:else if error && !draft}
		<div class="flex flex-1 items-center justify-center p-8">
			<EmptyState title="Could not load the definition" description={error}>
				{#snippet actions()}
					<Button variant="secondary" size="sm" onclick={() => void load()}>Retry</Button>
				{/snippet}
			</EmptyState>
		</div>
	{:else if draft}
		<div class="min-h-0 flex-1 overflow-auto">
			{#if error}
				<div class="px-2.5 pt-2">
					<Alert tone="danger">{error}</Alert>
				</div>
			{/if}
			{#if plan && plan.warnings.length > 0}
				<div class="space-y-1 px-2.5 pt-2">
					{#each plan.warnings as warning, i (i)}
						<Alert tone="warning">{warning}</Alert>
					{/each}
				</div>
			{/if}

			{#if limited && !collection}
				<p class="m-0 px-2.5 pt-2 text-caption text-fg-muted">
					Here you can add columns, rename the table or a column, and change indexes. Other column
					and foreign key changes are not available for this database yet.
				</p>
			{/if}

			{#if !collection}
				<section class="border-b border-line px-2.5 py-2">
					<div class="mb-2 flex items-center justify-between gap-2">
						<h2 class="m-0 text-label font-medium text-fg">Columns</h2>
						<Button variant="ghost" size="sm" onclick={() => draft && (draft = addColumn(draft))}>
							Add column
						</Button>
					</div>
					<div class="space-y-1.5">
						{#each draft.columns ?? [] as column, i (i)}
							<div class="grid grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)_auto_4.5rem_auto] items-center gap-1.5">
								{#if column.drop}
									<span class="truncate font-mono text-label text-fg-muted line-through">{column.name}</span>
									<span class="text-caption text-fg-muted">Dropped</span>
									<span></span>
									<span></span>
									<Button variant="ghost" size="sm" onclick={() => draft && (draft = restoreColumn(draft, i))}>
										Restore
									</Button>
								{:else}
									{@const locked = limited && !!column.original}
									<input class={field} bind:value={column.name} aria-label="Column name" />
									<input
										class={field}
										bind:value={column.dataType}
										aria-label="Column type"
										disabled={locked}
									/>
									<label class="inline-flex items-center gap-1 text-caption text-fg-secondary">
										<input type="checkbox" bind:checked={column.nullable} disabled={locked} />
										Null
									</label>
									<input
										class={field}
										type="number"
										min="0"
										value={column.pkPosition ?? 0}
										aria-label="Primary key position"
										disabled={limited}
										oninput={(e) => {
											const next = Number(e.currentTarget.value)
											column.pkPosition =
												Number.isFinite(next) && next > 0 ? Math.floor(next) : 0
										}}
									/>
									<Button
										variant="danger-ghost"
										size="sm"
										disabled={locked}
										onclick={() => draft && (draft = dropColumn(draft, i))}
									>
										Drop
									</Button>
								{/if}
							</div>
						{/each}
					</div>
				</section>
			{/if}

			<section class="border-b border-line px-2.5 py-2">
				<div class="mb-2 flex items-center justify-between gap-2">
					<h2 class="m-0 text-label font-medium text-fg">Indexes</h2>
					<Button variant="ghost" size="sm" onclick={() => draft && (draft = addIndex(draft))}>
						Add index
					</Button>
				</div>
				<div class="space-y-1.5">
					{#each draft.indexes ?? [] as index, i (i)}
						<div class="grid grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)_auto_auto_auto] items-center gap-1.5">
							{#if index.drop}
								<span class="truncate font-mono text-label text-fg-muted line-through">{index.name}</span>
								<span class="text-caption text-fg-muted">Dropped</span>
								<span></span>
								<span></span>
								<Button variant="ghost" size="sm" onclick={() => draft && (draft = restoreIndex(draft, i))}>
									Restore
								</Button>
							{:else}
								<input class={field} bind:value={index.name} aria-label="Index name" disabled={index.primary} />
								<input
									class={field}
									value={columnListText(index.columns)}
									aria-label="Index columns"
									disabled={index.primary}
									onchange={(e) => draft && (draft = setIndexColumns(draft, i, e.currentTarget.value))}
								/>
								<label class="inline-flex items-center gap-1 text-caption text-fg-secondary">
									<input type="checkbox" bind:checked={index.unique} disabled={index.primary} />
									Unique
								</label>
								<label class="inline-flex items-center gap-1 text-caption text-fg-secondary">
									<input
										type="checkbox"
										checked={!!index.concurrent}
										disabled={index.primary}
										onchange={(e) => (index.concurrent = e.currentTarget.checked)}
									/>
									No lock
								</label>
								{#if index.primary}
									<span class="text-caption text-fg-muted">Primary</span>
								{:else}
									<Button variant="danger-ghost" size="sm" onclick={() => draft && (draft = dropIndex(draft, i))}>
										Drop
									</Button>
								{/if}
							{/if}
						</div>
					{/each}
				</div>
			</section>

			{#if showForeignKeys && !collection && !limited}
				<section class="border-b border-line px-2.5 py-2">
					<div class="mb-2 flex items-center justify-between gap-2">
						<h2 class="m-0 text-label font-medium text-fg">Foreign keys</h2>
						<Button variant="ghost" size="sm" onclick={() => draft && (draft = addForeignKey(draft))}>
							Add foreign key
						</Button>
					</div>
					<div class="space-y-1.5">
						{#each draft.foreignKeys ?? [] as fk, i (i)}
							<div class="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_minmax(0,1fr)_minmax(0,1fr)_auto] items-center gap-1.5">
								{#if fk.drop}
									<span class="truncate font-mono text-label text-fg-muted line-through">{fk.name}</span>
									<span class="col-span-3 text-caption text-fg-muted">Dropped</span>
									<Button variant="ghost" size="sm" onclick={() => draft && (draft = restoreForeignKey(draft, i))}>
										Restore
									</Button>
								{:else}
									<input class={field} bind:value={fk.name} aria-label="Foreign key name" />
									<input
										class={field}
										value={columnListText(fk.columns)}
										aria-label="Foreign key columns"
										onchange={(e) =>
											draft && (draft = setForeignKeyColumns(draft, i, e.currentTarget.value, 'columns'))}
									/>
									<input class={field} bind:value={fk.refTable} aria-label="Referenced table" />
									<input
										class={field}
										value={columnListText(fk.refColumns)}
										aria-label="Referenced columns"
										onchange={(e) =>
											draft &&
											(draft = setForeignKeyColumns(draft, i, e.currentTarget.value, 'refColumns'))}
									/>
									<Button variant="danger-ghost" size="sm" onclick={() => draft && (draft = dropForeignKey(draft, i))}>
										Drop
									</Button>
								{/if}
							</div>
						{/each}
					</div>
				</section>
			{/if}

			{#if collection}
				<section class="border-b border-line px-2.5 py-2">
					<h2 class="m-0 mb-2 text-label font-medium text-fg">Validator</h2>
					<textarea
						class="min-h-24 w-full rounded-md border border-line bg-elevated px-2 py-1.5 font-mono text-label text-fg outline-none focus:border-line-strong"
						bind:value={validatorText}
						aria-label="Collection validator"
						spellcheck="false"
					></textarea>
				</section>
			{/if}

			<section class="px-2.5 py-2">
				<h2 class="m-0 mb-2 text-label font-medium text-fg">SQL</h2>
				{#if plan?.note}
					<p class="mt-0 mb-2 text-caption text-fg-muted">{plan.note}</p>
				{/if}
				{#if planError}
					<p class="m-0 text-caption text-danger">{planError}</p>
				{:else if plan && plan.statements.length > 0}
					<pre class="m-0 overflow-auto rounded-md border border-line bg-input p-2 font-mono text-label text-fg whitespace-pre-wrap">{plan.statements.join('\n')}</pre>
				{:else}
					<p class="m-0 text-caption text-fg-muted">No changes yet.</p>
				{/if}
			</section>
		</div>
	{/if}
</div>
