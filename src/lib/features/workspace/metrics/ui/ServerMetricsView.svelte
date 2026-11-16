<script lang="ts">
	/**
	 * Server metrics tab: overview tiles, live sessions, slow queries.
	 * Plugin-owned data; refreshes every 5 s while live and the window shows.
	 */
	import type {
		ActiveBackendSession,
		EngineCapabilities,
		ServerMetricsSnapshot,
	} from '#core/contracts'
	import { errorMessage } from '#core/ipc'
	import {
		Alert,
		Button,
		EmptyState,
		FetchedAgo,
		Icon,
		IconButton,
		Pill,
		SearchField,
		Select,
		StatusBar,
		StatusDot,
		type IconName,
		type SelectOption,
	} from '#core/ui'
	import { cn } from '#core/util'
	import { resolveCapabilities } from '#engines'
	import { showToast } from '#features/toast'
	import { fetchServerMetrics, terminateBackendSession } from '../api'
	import { backendSessionLabel } from '../presentation'
	import ActiveSessionsTable from './ActiveSessionsTable.svelte'
	import MetricsOverview from './MetricsOverview.svelte'
	import SlowQueriesTable from './SlowQueriesTable.svelte'
	import TerminateSessionModal from './TerminateSessionModal.svelte'

	interface Props {
		engine: string
		engineSessionId: string | null
		readOnly: boolean
		capabilities?: EngineCapabilities | null
		production?: boolean
	}

	let {
		engine,
		engineSessionId,
		readOnly,
		capabilities = null,
		production = false,
	}: Props = $props()

	type Pane = 'overview' | 'sessions' | 'slow'

	const LIVE_EVERY_MS = 5000

	const durationOptions: SelectOption[] = [
		{ value: '500', label: '≥ 500 ms' },
		{ value: '1000', label: '≥ 1 s' },
		{ value: '5000', label: '≥ 5 s' },
		{ value: '10000', label: '≥ 10 s' },
	]

	const canTerminate = $derived(
		!!resolveCapabilities(engine, capabilities).supportsTerminateSession && !readOnly,
	)

	let pane = $state<Pane>('overview')
	let live = $state(true)
	let minDurationMs = $state(1000)
	let snapshot = $state<ServerMetricsSnapshot | null>(null)
	let error = $state<string | null>(null)
	let loading = $state(false)
	let fetchedAt = $state<number | null>(null)
	let pending = $state<ActiveBackendSession | null>(null)
	let ending = $state(false)
	let filters = $state<Record<Exclude<Pane, 'overview'>, string>>({ sessions: '', slow: '' })

	const tabs = $derived<{ id: Pane; label: string; icon: IconName; count: number | null }[]>([
		{ id: 'overview', label: 'Overview', icon: 'metrics', count: null },
		{ id: 'sessions', label: 'Connections', icon: 'plug', count: snapshot?.sessions.length ?? null },
		{ id: 'slow', label: 'Slow queries', icon: 'history', count: snapshot?.slowQueries.length ?? null },
	])

	/** A newer request wins; an older reply that lands late is dropped. */
	let requestSeq = 0

	async function load(opts?: { quiet?: boolean }) {
		if (!engineSessionId) {
			error = 'This connection has no live engine session. Reconnect first.'
			return
		}
		const seq = ++requestSeq
		if (!opts?.quiet) loading = true
		try {
			const next = await fetchServerMetrics({
				sessionId: engineSessionId,
				includeSessions: true,
				includeSlowQueries: true,
				minDurationMs,
			})
			if (seq !== requestSeq) return
			snapshot = next
			error = null
			fetchedAt = Date.now()
		} catch (err) {
			if (seq === requestSeq) error = errorMessage(err)
		} finally {
			if (seq === requestSeq) loading = false
		}
	}

	async function confirmTerminate() {
		const row = pending
		if (!row || !engineSessionId || ending) return
		ending = true
		try {
			const result = await terminateBackendSession({
				sessionId: engineSessionId,
				backendSessionId: row.id,
			})
			if (result.ok) {
				showToast('success', result.message || `Ended session ${row.id}.`)
				pending = null
				await load({ quiet: true })
			} else {
				showToast('danger', result.message || `Could not end session ${row.id}.`)
			}
		} catch (err) {
			showToast('danger', errorMessage(err))
		} finally {
			ending = false
		}
	}

	/**
	 * Panes stay mounted after their first visit and are only hidden, so a
	 * switch costs nothing: no rebuild, scroll kept, the entry motion once.
	 */
	let visited = $state<Record<Pane, boolean>>({ overview: true, sessions: false, slow: false })

	/** A refresh the user asked for dims the pane; quiet live polls do not. */
	const paneClass = $derived(
		cn(
			'flex min-h-0 min-w-0 flex-1 flex-col transition-opacity duration-150',
			loading && 'opacity-60',
		),
	)

	function selectPane(id: Pane) {
		visited[id] = true
		pane = id
	}

	function onTabKeydown(e: KeyboardEvent) {
		const ids = tabs.map((t) => t.id)
		const i = ids.indexOf(pane)
		const next =
			e.key === 'ArrowRight' ? ids[Math.min(ids.length - 1, i + 1)]
			: e.key === 'ArrowLeft' ? ids[Math.max(0, i - 1)]
			: e.key === 'Home' ? ids[0]
			: e.key === 'End' ? ids[ids.length - 1]
			: null
		if (!next) return
		e.preventDefault()
		selectPane(next)
		document.getElementById(`metrics-tab-${next}`)?.focus()
	}

	$effect(() => {
		void engineSessionId
		void minDurationMs
		void load()
	})

	$effect(() => {
		if (!live || !engineSessionId) return
		const id = window.setInterval(() => {
			if (!document.hidden) void load({ quiet: true })
		}, LIVE_EVERY_MS)
		return () => window.clearInterval(id)
	})
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col bg-app">
	<header class="flex h-12 shrink-0 items-center gap-2.5 border-b border-line bg-surface pr-3 pl-4">
		<div class="flex min-w-0 items-center gap-2">
			<span class="flex shrink-0 text-accent-text [&_svg]:size-4" aria-hidden="true">
				<Icon name="metrics" />
			</span>
			<h1 class="m-0 truncate text-[0.84375rem] font-medium text-fg">Server metrics</h1>
			<Pill uppercase>{engine}</Pill>
		</div>

		<span class="min-w-0 flex-1"></span>

		{#if pane === 'slow'}
			<!-- Select fills its parent (`w-full`); the wrapper sets the width. -->
			<div class="w-28 shrink-0">
				<Select
					value={String(minDurationMs)}
					options={durationOptions}
					inputSize="sm"
					ariaLabel="Slower than"
					onchange={(v) => (minDurationMs = Number(v) || 1000)}
				/>
			</div>
		{/if}
		{#if pane !== 'overview'}
			<div class="w-55 min-w-0 shrink">
				<SearchField
					bind:value={filters[pane]}
					id="metrics-filter"
					placeholder={pane === 'sessions' ? 'Filter connections…' : 'Filter queries…'}
					label={pane === 'sessions' ? 'Filter connections' : 'Filter queries'}
				/>
			</div>
		{/if}
	</header>

	<div
		class="scrollbar-none flex h-10.5 shrink-0 items-stretch gap-0.5 overflow-x-auto border-b border-line bg-surface px-2.5"
		role="tablist"
		aria-label="Metrics sections"
	>
		{#each tabs as tab (tab.id)}
			{@const active = pane === tab.id}
			<button
				type="button"
				role="tab"
				id="metrics-tab-{tab.id}"
				aria-controls="metrics-pane-{tab.id}"
				aria-selected={active}
				tabindex={active ? 0 : -1}
				class={cn(
					'relative inline-flex shrink-0 items-center gap-[7px] border-none bg-transparent px-[11px] text-[0.78125rem] font-medium transition-colors duration-150',
					active ? 'text-fg' : 'text-fg-muted hover:text-fg',
				)}
				onclick={() => selectPane(tab.id)}
				onkeydown={onTabKeydown}
			>
				<span
					class={cn('flex [&_svg]:size-3.5', active ? 'text-accent-text' : 'text-fg-muted')}
					aria-hidden="true"
				>
					<Icon name={tab.icon} />
				</span>
				{tab.label}
				{#if tab.count != null}
					<span
						class={cn(
							'rounded-[5px] px-1.5 py-px font-mono text-[0.65625rem] tabular-nums',
							active ? 'bg-accent-soft text-accent-text' : 'bg-hover text-fg-muted',
						)}
					>
						{tab.count}
					</span>
				{/if}
				{#if active}
					<span class="absolute inset-x-2.5 -bottom-px h-0.5 rounded-full bg-accent" aria-hidden="true"></span>
				{/if}
			</button>
		{/each}
	</div>

	{#if error && snapshot}
		<div class="shrink-0 px-2.5 pt-2">
			<Alert tone="danger">
				<div class="flex items-center justify-between gap-2">
					<span class="min-w-0 truncate" title={error}>{error}</span>
					<Button variant="ghost" size="sm" onclick={() => void load()}>Retry</Button>
				</div>
			</Alert>
		</div>
	{/if}

	{#if error && !snapshot}
		<div class="flex flex-1 items-center justify-center p-8">
			<EmptyState title="Could not load metrics" description={error}>
				{#snippet actions()}
					<Button variant="secondary" size="sm" onclick={() => void load()}>Retry</Button>
				{/snippet}
			</EmptyState>
		</div>
	{:else if !snapshot}
		<div class="min-h-0 flex-1" aria-busy="true">
			<div class="grid grid-cols-[repeat(auto-fill,minmax(13rem,1fr))] gap-3 p-4">
				{#each [1, 2, 3, 4, 5] as i (i)}
					<div class="h-28 animate-pulse rounded-lg border border-line bg-surface"></div>
				{/each}
			</div>
		</div>
	{:else}
		<div
			id="metrics-pane-overview"
			role="tabpanel"
			aria-labelledby="metrics-tab-overview"
			aria-busy={loading}
			hidden={pane !== 'overview'}
			class={paneClass}
		>
			<MetricsOverview {snapshot} />
		</div>
		{#if visited.sessions}
			<div
				id="metrics-pane-sessions"
				role="tabpanel"
				aria-labelledby="metrics-tab-sessions"
				aria-busy={loading}
				hidden={pane !== 'sessions'}
				class={paneClass}
			>
				<ActiveSessionsTable
					sessions={snapshot.sessions}
					bind:filter={filters.sessions}
					supported={snapshot.capabilities.activeSessions}
					{canTerminate}
					onend={(row) => (pending = row)}
				/>
			</div>
		{/if}
		{#if visited.slow}
			<div
				id="metrics-pane-slow"
				role="tabpanel"
				aria-labelledby="metrics-tab-slow"
				aria-busy={loading}
				hidden={pane !== 'slow'}
				class={paneClass}
			>
				<SlowQueriesTable
					queries={snapshot.slowQueries}
					bind:filter={filters.slow}
					supported={snapshot.capabilities.slowQueries}
					{minDurationMs}
				/>
			</div>
		{/if}
	{/if}

	<StatusBar class="pl-1.5!" ariaLabel="Metrics status">
		{#snippet left()}
			<button
				type="button"
				class="inline-flex h-6 items-center gap-1.5 rounded-sm border-none bg-transparent px-2 text-label text-fg-muted transition-colors duration-150 hover:bg-hover hover:text-fg"
				aria-pressed={live}
				title={live ? 'Pause live refresh' : 'Refresh every 5 s'}
				onclick={() => (live = !live)}
			>
				<StatusDot status={live ? 'connected' : 'idle'} label={live ? 'Live' : 'Paused'} />
				{live ? 'Live' : 'Paused'}
			</button>
		{/snippet}
		{#snippet right()}
			{#if fetchedAt != null}
				<FetchedAgo at={fetchedAt} class="mr-1" />
			{/if}
			<IconButton
				label={loading ? 'Refreshing metrics' : 'Refresh metrics'}
				size="sm"
				variant="quiet"
				disabled={loading || !engineSessionId}
				aria-busy={loading}
				onclick={() => void load()}
				class={loading ? 'opacity-45' : ''}
			>
				<Icon name="refresh" class={cn(loading && 'animate-spin')} />
			</IconButton>
		{/snippet}
	</StatusBar>
</div>

<TerminateSessionModal
	open={pending != null}
	sessionLabel={pending ? backendSessionLabel(pending) : ''}
	queryPreview={pending?.query ?? null}
	{production}
	busy={ending}
	onconfirm={() => void confirmTerminate()}
	oncancel={() => {
		if (!ending) pending = null
	}}
/>
