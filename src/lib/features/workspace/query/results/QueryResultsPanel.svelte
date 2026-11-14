<script lang="ts">
	import type { QueryResult, StatementResult } from '#core/contracts'
	import type { ResultColumnSource } from '#engines'
	import {
		Button,
		ContextMenu,
		Icon,
		IconButton,
		Pager,
		Spinner,
		type ContextMenuItem,
	} from '#core/ui'
	import { cn, copyTextToClipboard } from '#core/util'
	import { isMacPlatform, keyCap } from '#features/shortcuts'
	import { StatusNote } from '#features/toast'
	import { statementChipLabel, statementChipTitle } from './batch-view'
	import type { RunState } from '../run/run-types'
	import QueryResultsGrid from '../ui/QueryResultsGrid.svelte'
	import type { LargeLoader } from '#features/workspace/data-grid'

	export type ResultExportAction =
		| 'download-csv'
		| 'download-json'
		| 'download-excel'
		| 'copy-csv'
		| 'copy-json'

	interface Props {
		runState: RunState
		resultColumns: QueryResult['columns']
		/** Source table column per result column, when known (header tooltip). */
		resultSources?: (ResultColumnSource | null)[]
		resultPageRows: unknown[][]
		resultTotal: number
		resultPageStart: number
		resultPageEnd: number
		resultSafePage: number
		resultTotalPages: number
		canExport: boolean
		exporting: ResultExportAction | null
		refreshing: boolean
		onexport: (action: ResultExportAction) => void
		onprev: () => void
		onnext: () => void
		onrefresh: () => void
		onaskai?: (message: string) => void
		canLocate?: boolean
		onlocate?: () => void
		statements?: StatementResult[]
		activeIndex?: number
		submittedText?: string
		onselectstatement?: (index: number) => void
		identStyle?: 'postgres' | 'mysql'
		/** Loads a large value in full (the tab's session, through its transaction). */
		loadLarge?: LargeLoader | null
	}

	let {
		runState,
		resultColumns,
		resultSources = [],
		resultPageRows,
		resultTotal,
		resultPageStart,
		resultPageEnd,
		resultSafePage,
		resultTotalPages,
		canExport,
		exporting,
		refreshing,
		onexport,
		onprev,
		onnext,
		onrefresh,
		onaskai,
		canLocate = false,
		onlocate,
		statements = [],
		activeIndex = 0,
		submittedText = '',
		onselectstatement,
		identStyle = 'postgres',
		loadLarge = null,
	}: Props = $props()

	let menu = $state<{ x: number; y: number } | null>(null)
	let copiedError = $state(false)
	let copiedErrorTimer: ReturnType<typeof setTimeout> | null = null


	const exportItems: ContextMenuItem[] = [
		{ id: 'download-csv', label: 'Download as CSV', icon: 'download' },
		{ id: 'download-json', label: 'Download as JSON', icon: 'download' },
		{ id: 'download-excel', label: 'Download as Excel', icon: 'download' },
		{
			id: 'copy-csv',
			label: 'Copy as CSV',
			icon: 'copy',
			separatorBefore: true,
		},
		{ id: 'copy-json', label: 'Copy as JSON', icon: 'copy' },
	]

	const hasRows = $derived(
		runState.status === 'ok' && resultTotal > 0 && resultColumns.length > 0,
	)
	const errorParts = $derived.by(() => {
		if (runState.status !== 'error' && runState.status !== 'blocked') {
			return { body: '', hint: null as string | null }
		}
		const raw = runState.message.trim()
		const hit = raw.match(/^(.*?)\s+Hint:\s+(.+)$/s)
		if (!hit) return { body: raw, hint: null }
		return { body: hit[1]!.trim(), hint: hit[2]!.trim() }
	})
	const activeStatement = $derived(statements[activeIndex] ?? null)
	const notices = $derived(activeStatement?.notices ?? [])
	const activeAffected = $derived(activeStatement?.affectedRows ?? null)
	const mac = isMacPlatform()
	const runChord = mac ? `${keyCap('Mod', 'glyphs', mac)}↵` : 'Ctrl+Enter'

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
		menu = { x: rect.right, y: rect.top - 4 }
	}

	async function copyError(message: string) {
		const result = await copyTextToClipboard(message)
		if (!result.ok) return
		copiedError = true
		if (copiedErrorTimer) clearTimeout(copiedErrorTimer)
		copiedErrorTimer = setTimeout(() => {
			copiedError = false
		}, 1600)
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

{#snippet failure(title: string, tone: 'danger' | 'warning', message: string, hint: string | null)}
	<div
		class="m-3 overflow-hidden rounded-lg bg-surface shadow-[inset_0_0_0_1px_var(--line)]"
		role="alert"
	>
		<div class="flex items-center gap-2 py-2 pr-2 pl-3.5">
			<span class={cn('flex [&_svg]:size-4', tone === 'danger' ? 'text-danger' : 'text-warning')}>
				<Icon name={tone === 'danger' ? 'alert-circle' : 'shield-alert'} />
			</span>
			<p class="m-0 min-w-0 flex-1 truncate font-medium">{title}</p>
			<IconButton
				label={copiedError ? 'Copied' : 'Copy error'}
				size="sm"
				variant="quiet"
				onclick={() => void copyError(message)}
			>
				<Icon name={copiedError ? 'check' : 'copy'} />
			</IconButton>
			{#if canLocate && onlocate}
				<Button size="sm" variant="ghost" onclick={onlocate}>Show in editor</Button>
			{/if}
			{#if onaskai}
				<Button size="sm" onclick={() => onaskai(message)}>
					<span class="flex text-accent-text"><Icon name="sparkle" /></span>Fix with agent
				</Button>
			{/if}
		</div>
		{#if message}
			<pre
				class="m-0 max-h-48 overflow-auto border-t border-line bg-field px-3.5 py-3 font-mono text-label leading-relaxed whitespace-pre-wrap text-fg select-text">{message}</pre>
		{/if}
		{#if hint}
			<p class="m-0 border-t border-line px-3.5 py-2.5 text-label text-fg-secondary">
				<span class="font-medium text-fg">Hint</span> · {hint}
			</p>
		{/if}
	</div>
{/snippet}

{#snippet quiet(title: string, body: string)}
	<div class="flex flex-1 flex-col items-center justify-center gap-1 px-6 py-8 text-center">
		<p class="m-0 font-medium text-fg-secondary">{title}</p>
		<p class="m-0 max-w-prose-ui text-label text-fg-muted">{body}</p>
	</div>
{/snippet}

<div class="flex min-h-0 min-w-0 flex-1 flex-col bg-app">

	{#if runState.status === 'ok' && notices.length > 0}
		<div class="shrink-0 border-b border-line bg-info-soft px-3.5 py-2">
			{#each notices as notice, i (i)}
				<p class="m-0 text-label leading-relaxed text-fg-secondary">{notice}</p>
			{/each}
		</div>
	{/if}

	<div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
		{#if runState.status === 'idle'}
			<div class="flex flex-1 flex-col items-center justify-center gap-2 px-6 py-8 text-center">
				<p class="m-0 font-medium text-fg-secondary">Run a query to see results</p>
				<p class="m-0 flex items-center gap-1.5 text-label text-fg-muted">
					<kbd>{runChord}</kbd> runs the statement at the cursor
				</p>
			</div>
		{:else if runState.status === 'running'}
			<div
				class="flex flex-1 flex-col items-center justify-center gap-2 px-6 py-8 text-center text-fg-muted"
				aria-busy="true"
			>
				<Spinner />
				<p class="m-0 text-label">
					{#if runState.progress && runState.progress.total > 1}
						Statement {runState.progress.index + 1} of {runState.progress.total}…
					{:else}
						Running…
					{/if}
				</p>
			</div>
		{:else if runState.status === 'ok'}
			{#if activeStatement?.error}
				{@render failure(
					`Statement ${statementChipLabel(statements, activeStatement)} failed`,
					'danger',
					activeStatement.error,
					null,
				)}
			{:else if runState.result && runState.result.columns.length > 0 && resultTotal > 0}
				<QueryResultsGrid
					columns={resultColumns}
					sources={resultSources}
					rows={resultPageRows}
					pageStart={resultPageStart}
					{identStyle}
					{loadLarge}
				/>
			{:else if runState.result && runState.result.columns.length > 0}
				{@render quiet('No rows', 'The query returned an empty result.')}
			{:else}
				{@render quiet(
					'Done',
					runState.result?.message ?? 'The statement finished without a result set.',
				)}
			{/if}
		{:else if runState.status === 'blocked' || runState.status === 'error'}
			{@render failure(
				runState.status === 'blocked' ? 'Query blocked' : 'Query failed',
				runState.status === 'blocked' ? 'warning' : 'danger',
				errorParts.body,
				errorParts.hint,
			)}
		{/if}
	</div>
	<!-- Under the rows. Left: what ran and how much came back · centre: pages · right: actions. -->
	<div
		class="grid h-statusbar shrink-0 grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-3 border-t border-line bg-surface pr-1.5 pl-2"
		aria-label="Results"
		role="group"
	>
		<div class="flex min-w-0 items-center gap-2.5">
			{#if runState.status === 'ok' && statements.length > 1}
				<div class="flex min-w-0 shrink gap-0.5 overflow-x-auto scrollbar-none" role="tablist" aria-label="Result sets">
					{#each statements as stmt, i (i)}
						<button
							type="button"
							role="tab"
							aria-selected={i === activeIndex}
							title={statementChipTitle(submittedText, stmt.sqlRange)}
							class={cn(
								'h-7 shrink-0 rounded-[7px] border-none px-2 font-mono text-label tabular-nums transition-colors',
								i === activeIndex
									? 'bg-hover shadow-[inset_0_0_0_1px_var(--line-2)]'
									: 'bg-transparent hover:bg-hover',
								stmt.error ? 'text-danger' : i === activeIndex ? 'text-fg' : 'text-fg-muted',
							)}
							onclick={() => onselectstatement?.(i)}
						>
							{statementChipLabel(statements, stmt)}
						</button>
					{/each}
				</div>
			{:else}
				<span class="shrink-0 px-1 text-label font-medium text-fg-secondary">Results</span>
			{/if}
			<div class="flex min-w-0 items-center gap-2.5 font-mono text-label text-fg-muted tabular-nums">
				{#if hasRows}
					<span class="shrink-0 text-fg-secondary">
						{resultPageStart + 1}–{resultPageEnd} of {resultTotal.toLocaleString()}
					</span>
				{:else if runState.status === 'ok' && resultColumns.length > 0}
					<span class="shrink-0">0 rows</span>
				{/if}
				{#if runState.status === 'ok' && activeAffected != null && resultColumns.length > 0}
					<span class="shrink-0">{activeAffected.toLocaleString()} affected</span>
				{/if}
				{#if durationMs != null}
					<span class="shrink-0" title={`Ran in ${durationMs.toLocaleString()} ms`}>
						{formatDuration(durationMs)}
					</span>
				{/if}
				{#if runState.status === 'ok' && runState.message}
					<span class="min-w-0 truncate font-sans" title={runState.message}>{runState.message}</span>
				{/if}
				<StatusNote />
			</div>
		</div>

		<div class="flex justify-center">
			{#if hasRows && resultTotalPages > 1}
				<Pager page={resultSafePage} totalPages={resultTotalPages} active disabled={false} {onprev} {onnext} />
			{/if}
		</div>

		<div class="flex items-center justify-end gap-0.5">
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
			<IconButton
				label={refreshing ? 'Running…' : 'Run again'}
				size="sm"
				variant="quiet"
				disabled={refreshing || runState.status === 'idle'}
				aria-busy={refreshing}
				onclick={onrefresh}
			>
				<Icon name="refresh" class={cn(refreshing && 'animate-spin')} />
			</IconButton>
		</div>
	</div>
</div>

{#if menu}
	<ContextMenu
		x={menu.x}
		y={menu.y}
		items={exportItems}
		placement="above-end"
		ariaLabel="Download or copy results"
		onpick={onMenuPick}
		onclose={() => (menu = null)}
	/>
{/if}
