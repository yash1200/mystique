<script lang="ts">
	import { errorMessage } from '#core/ipc'
	import type {
		ActiveBackendSession,
		EngineCapabilities,
		ServerMetricsSnapshot,
	} from '#core/contracts'
	import { Button, EmptyState } from '#core/ui'
	import { cn } from '#core/util'
	import { resolveCapabilities } from '#engines'
	import { showToast } from '#features/toast'
	import { fetchServerMetrics, terminateBackendSession } from '../api'
	import { backendSessionLabel } from '../presentation'
	import type { MetricsPane } from '../types'
	import ActiveSessionsTable from './ActiveSessionsTable.svelte'
	import MetricsHeader from './MetricsHeader.svelte'
	import MetricsOverview from './MetricsOverview.svelte'
	import MetricsStatusBar from './MetricsStatusBar.svelte'
	import MetricsToolbar from './MetricsToolbar.svelte'
	import SlowQueriesTable from './SlowQueriesTable.svelte'
	import TerminateSessionModal from './TerminateSessionModal.svelte'

	interface Props {
		engine: string
		engineSessionId: string | null
		profileReadOnly: boolean
		capabilities?: EngineCapabilities | null
		production?: boolean
	}

	let {
		engine,
		engineSessionId,
		profileReadOnly,
		capabilities = null,
		production = false,
	}: Props = $props()

	const caps = $derived(resolveCapabilities(engine, capabilities))
	const canTerminate = $derived(
		!!caps.supportsTerminateSession && !profileReadOnly,
	)

	let pane = $state<MetricsPane>('overview')
	let live = $state(true)
	let minDurationMs = $state(1000)
	let snapshot = $state<ServerMetricsSnapshot | null>(null)
	let error = $state<string | null>(null)
	let loading = $state(false)
	let fetchedAt = $state<number | null>(null)
	let now = $state(Date.now())
	let pending = $state<ActiveBackendSession | null>(null)
	let ending = $state(false)
	let sessionFilter = $state('')
	let slowFilter = $state('')

	const skeletonSpans = [
		'col-span-12 sm:col-span-6 lg:col-span-5',
		'col-span-6 lg:col-span-3',
		'col-span-6 lg:col-span-4',
		'col-span-12 sm:col-span-7',
		'col-span-12 sm:col-span-5',
	]

	async function load(opts?: { quiet?: boolean }) {
		if (!engineSessionId) {
			error = 'This connection has no live engine session. Reconnect first.'
			return
		}
		if (!opts?.quiet) loading = true
		try {
			const next = await fetchServerMetrics({
				sessionId: engineSessionId,
				includeSessions: true,
				includeSlowQueries: true,
				minDurationMs,
			})
			snapshot = next
			error = null
			fetchedAt = Date.now()
		} catch (err) {
			error = errorMessage(err)
		} finally {
			loading = false
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
				profileReadOnly,
			})
			if (result.ok) {
				showToast('success', result.message || `Ended session ${row.id}.`)
				pending = null
				await load({ quiet: true })
			} else {
				showToast(
					'danger',
					result.message || `Could not end session ${row.id}.`,
				)
			}
		} catch (err) {
			showToast('danger', errorMessage(err))
		} finally {
			ending = false
		}
	}

	$effect(() => {
		void engineSessionId
		void minDurationMs
		void load()
	})

	$effect(() => {
		if (!live || !engineSessionId) return
		const id = window.setInterval(() => {
			if (document.hidden) return
			void load({ quiet: true })
		}, 5000)
		return () => window.clearInterval(id)
	})

	$effect(() => {
		const id = window.setInterval(() => {
			now = Date.now()
		}, 15_000)
		return () => window.clearInterval(id)
	})
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash">
	<MetricsHeader
		{live}
		{loading}
		canRefresh={!!engineSessionId}
		onlive={(v) => (live = v)}
		onrefresh={() => void load()}
	/>
	<MetricsToolbar
		{pane}
		{minDurationMs}
		bind:sessionFilter
		bind:slowFilter
		onpane={(v) => (pane = v)}
		onduration={(ms) => (minDurationMs = ms)}
	/>

	<div class="min-h-0 flex-1 overflow-auto p-4">
		{#if error && !snapshot}
			<div class="mx-auto max-w-panel-sm py-10">
				<EmptyState title="Could not load metrics" description={error}>
					{#snippet actions()}
						<Button size="sm" onclick={() => void load()}>Retry</Button>
					{/snippet}
				</EmptyState>
			</div>
		{:else if loading && !snapshot}
			<div aria-busy="true">
				<p class="mb-3 text-caption text-fg-muted">Loading…</p>
				<div class="grid grid-cols-12 gap-3">
					{#each skeletonSpans as span (span)}
						<div
							class={cn(
								'h-32 animate-pulse rounded-xl border border-line bg-elevated',
								span,
							)}
						></div>
					{/each}
				</div>
			</div>
		{:else if snapshot && pane === 'overview'}
			<MetricsOverview {snapshot} {error} />
		{:else if snapshot && pane === 'sessions'}
			<ActiveSessionsTable
				sessions={snapshot.sessions}
				filter={sessionFilter}
				supported={snapshot.capabilities.activeSessions}
				{canTerminate}
				onend={(row) => (pending = row)}
			/>
		{:else if snapshot && pane === 'slow'}
			<SlowQueriesTable
				queries={snapshot.slowQueries}
				filter={slowFilter}
				supported={snapshot.capabilities.slowQueries}
				{minDurationMs}
			/>
		{/if}
	</div>

	<MetricsStatusBar {fetchedAt} {now} {live} {snapshot} />
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
