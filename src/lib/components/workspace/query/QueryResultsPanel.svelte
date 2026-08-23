<script lang="ts">
	import type { QueryResult } from '$lib/api/engines'
	import {
		Badge,
		ContextMenu,
		Icon,
		IconButton,
		Kbd,
		Pager,
		StatusBar,
		type ContextMenuItem,
	} from '$lib/components/core'
	import { isMacPlatform, labelForKey } from '$lib/data/appShortcuts'
	import { appPrefs } from '$lib/stores/app-prefs.svelte'
	import { cn } from '$lib/utils/cn'
	import QueryResultsGrid from './QueryResultsGrid.svelte'
	import type { RunState } from './types'

	export type ResultExportAction =
		| 'download-csv'
		| 'download-json'
		| 'download-excel'
		| 'copy-csv'
		| 'copy-json'

	interface Props {
		runState: RunState
		resultColumns: QueryResult['columns']
		resultPageRows: unknown[][]
		resultTotal: number
		resultPageStart: number
		resultPageEnd: number
		resultSafePage: number
		resultTotalPages: number
		canExport: boolean
		exporting: ResultExportAction | null
		exportMsg: string | null
		refreshing: boolean
		onexport: (action: ResultExportAction) => void
		onprev: () => void
		onnext: () => void
		onrefresh: () => void
	}

	let {
		runState,
		resultColumns,
		resultPageRows,
		resultTotal,
		resultPageStart,
		resultPageEnd,
		resultSafePage,
		resultTotalPages,
		canExport,
		exporting,
		exportMsg,
		refreshing,
		onexport,
		onprev,
		onnext,
		onrefresh,
	}: Props = $props()

	let menu = $state<{ x: number; y: number } | null>(null)
	let gridMsg = $state<string | null>(null)
	let gridMsgTimer: ReturnType<typeof setTimeout> | null = null

	function onGridStatus(message: string) {
		gridMsg = message
		if (gridMsgTimer) clearTimeout(gridMsgTimer)
		gridMsgTimer = setTimeout(() => {
			if (gridMsg === message) gridMsg = null
		}, 1600)
	}

	const exportItems: ContextMenuItem[] = [
		{ id: 'dl-label', label: 'Download', heading: true },
		{ id: 'download-csv', label: 'CSV' },
		{ id: 'download-json', label: 'JSON' },
		{ id: 'download-excel', label: 'Excel' },
		{ id: 'copy-label', label: 'Copy', heading: true, separatorBefore: true },
		{ id: 'copy-csv', label: 'Copy as CSV' },
		{ id: 'copy-json', label: 'Copy as JSON' },
	]

	const statusText = $derived.by(() => {
		if (runState.status === 'idle') return 'No query run yet'
		if (runState.status === 'running') return 'Running…'
		if (
			runState.status === 'ok' ||
			runState.status === 'blocked' ||
			runState.status === 'error'
		) {
			return runState.message
		}
		return ''
	})

	const statusTone = $derived(
		runState.status === 'ok'
			? 'text-success'
			: runState.status === 'blocked' || runState.status === 'error'
				? 'text-danger'
				: 'text-fg-muted',
	)

	const hasRows = $derived(
		runState.status === 'ok' && resultTotal > 0 && resultColumns.length > 0,
	)
	const mac = isMacPlatform()
	const runMod = labelForKey('mod', mac)

	const durationMs = $derived.by(() => {
		if (
			runState.status !== 'ok' &&
			runState.status !== 'blocked' &&
			runState.status !== 'error'
		) {
			return null
		}
		const engineMs =
			runState.status === 'ok' ? runState.result?.durationMs : undefined
		const ms = engineMs ?? runState.elapsedMs
		return Number.isFinite(ms) ? ms : null
	})

	function formatDuration(ms: number): string {
		if (ms < 1000) return `${ms} ms`
		if (ms < 10_000) return `${(ms / 1000).toFixed(2)} s`
		if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`
		const totalSec = Math.round(ms / 1000)
		const m = Math.floor(totalSec / 60)
		const s = totalSec % 60
		return `${m}m ${s}s`
	}

	function toggleMenu(e?: MouseEvent) {
		if (!canExport || exporting !== null || refreshing) return
		if (menu) {
			menu = null
			return
		}
		const t = e?.currentTarget
		if (!(t instanceof HTMLElement)) return
		const rect = t.getBoundingClientRect()
		menu = { x: rect.right, y: rect.top }
	}

	function onMenuPick(action: string) {
		menu = null
		if (
			action === 'download-csv' ||
			action === 'download-json' ||
			action === 'download-excel' ||
			action === 'copy-csv' ||
			action === 'copy-json'
		) {
			onexport(action)
		}
	}
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash">
	<div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
		{#if runState.status === 'idle'}
			<div
				class="flex flex-1 flex-col items-center justify-center px-6 py-8 text-center"
			>
				<span
					class="mb-2 grid size-8 place-items-center rounded-lg bg-input"
					aria-hidden="true"
				>
					<Icon name="code" class="size-4 text-fg-muted" />
				</span>
				<p class="m-0 text-label font-medium text-fg">No query run yet</p>
				<p
					class="mt-1 mb-0 flex items-center gap-1 text-caption text-fg-muted"
				>
					Press
					<Kbd>{runMod}</Kbd>
					<Kbd>Enter</Kbd>
					to run
				</p>
			</div>
		{:else if runState.status === 'running'}
			<div
				class="flex flex-1 flex-col items-center justify-center px-6 py-8 text-center"
				aria-busy="true"
			>
				<span
					class="mb-2 grid size-8 place-items-center rounded-lg bg-input"
					aria-hidden="true"
				>
					<Icon name="refresh" class="size-4 animate-spin text-fg-muted" />
				</span>
				<p class="m-0 text-label font-medium text-fg">Running</p>
				<p class="mt-1 mb-0 text-caption text-fg-muted">
					Waiting for the engine.
				</p>
			</div>
		{:else if runState.status === 'ok' && runState.result}
			{@const result = runState.result}
			{#if result.columns.length === 0}
				<div
					class="flex flex-1 flex-col items-center justify-center px-6 py-8 text-center"
				>
					<p class="m-0 text-label font-medium text-fg">No tabular rows</p>
					<p class="mt-1 mb-0 max-w-prose-ui text-caption text-fg-muted">
						{result.message ?? 'The statement finished without a result set.'}
					</p>
				</div>
			{:else if resultTotal === 0}
				<div
					class="flex flex-1 flex-col items-center justify-center px-6 py-8 text-center"
				>
					<p class="m-0 text-label font-medium text-fg">No rows</p>
					<p class="mt-1 mb-0 text-caption text-fg-muted">
						The query returned an empty result.
					</p>
				</div>
			{:else}
				<QueryResultsGrid
					columns={resultColumns}
					rows={resultPageRows}
					pageStart={resultPageStart}
					onstatus={onGridStatus}
				/>
			{/if}
		{:else}
			<div
				class="m-3 rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-label text-danger"
				role="status"
			>
				{runState.status === 'blocked' || runState.status === 'error'
					? runState.message
					: ''}
			</div>
		{/if}
	</div>

	<StatusBar class="relative z-20" ariaLabel="Query results status">
		{#snippet left()}
			{#if hasRows}
				<span class="truncate font-mono tabular-nums">
					{resultPageStart + 1}–{resultPageEnd} of {resultTotal.toLocaleString()}
				</span>
			{:else}
				<span class="truncate font-mono tabular-nums">
					{runState.status === 'ok' && resultColumns.length > 0
						? '0 rows'
						: runState.status === 'idle'
							? 'Idle'
							: runState.status === 'running'
								? 'Running'
								: '0 rows'}
				</span>
			{/if}
			{#if durationMs != null}
				<span title={`Ran in ${durationMs.toLocaleString()} ms`}>
					<Badge tone="neutral" size="sm">{formatDuration(durationMs)}</Badge>
				</span>
			{/if}
			{#if runState.status === 'ok' && runState.result?.truncated}
				<Badge tone="warning" size="sm">truncated</Badge>
			{/if}
			{#if statusText && runState.status !== 'ok'}
				<span class={cn('min-w-0 truncate', statusTone)} title={statusText}>
					{statusText}
				</span>
			{:else if runState.status === 'ok' && runState.message}
				<span class="min-w-0 truncate text-fg-muted" title={runState.message}
					>{runState.message}</span
				>
			{/if}
			{#if exportMsg}
				<span class="shrink-0 truncate text-success" title={exportMsg}
					>· {exportMsg}</span
				>
			{/if}
			{#if gridMsg}
				<span class="shrink-0 truncate text-success" title={gridMsg}
					>· {gridMsg}</span
				>
			{/if}
		{/snippet}
		{#snippet center()}
			<Pager
				page={resultSafePage}
				totalPages={resultTotalPages}
				active={hasRows}
				disabled={!hasRows}
				{onprev}
				{onnext}
			/>
		{/snippet}
		{#snippet right()}
			<IconButton
				label={exporting ? 'Exporting…' : 'Download or copy results'}
				size="sm"
				variant="quiet"
				active={menu != null}
				disabled={!canExport || exporting !== null || refreshing}
				aria-haspopup="menu"
				aria-expanded={menu != null}
				onclick={(e) => toggleMenu(e)}
			>
				<Icon name="download" />
			</IconButton>
			{#if menu}
				<ContextMenu
					x={menu.x}
					y={menu.y}
					items={exportItems}
					ariaLabel="Download or copy results"
					onpick={onMenuPick}
					onclose={() => (menu = null)}
				/>
			{/if}
			<IconButton
				label={refreshing ? 'Refreshing…' : 'Re-run query'}
				size="sm"
				variant="quiet"
				disabled={refreshing || runState.status === 'idle'}
				aria-busy={refreshing}
				onclick={onrefresh}
				class={refreshing ? 'opacity-45' : ''}
			>
				<Icon name="refresh" class={cn(refreshing && 'animate-spin')} />
			</IconButton>
			<IconButton
				label={appPrefs.agentChatOpen ? 'Close agent' : 'Open agent'}
				size="sm"
				variant="quiet"
				active={appPrefs.agentChatOpen}
				onclick={() => appPrefs.setAgentChatOpen(!appPrefs.agentChatOpen)}
			>
				<Icon name="chat" />
			</IconButton>
		{/snippet}
	</StatusBar>
</div>
