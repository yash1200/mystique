<script lang="ts">
	import type { QueryResult, StatementResult } from '#core/contracts'
	import {
		Badge,
		Button,
		ContextMenu,
		Icon,
		IconButton,
		Kbd,
		Pager,
		StatusBar,
		type ContextMenuItem,
	} from '#core/ui'
	import { cn, copyTextToClipboard } from '#core/util'
	import { isMacPlatform, labelForKey } from '#features/shortcuts'
	import { StatusNote } from '#features/toast'
	import { statementChipLabel, statementChipTitle } from '../batch-view'
	import type { RunState } from '../run-types'
	import QueryResultsGrid from './QueryResultsGrid.svelte'

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
		menu = { x: rect.right, y: rect.top - 6 }
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
				<p class="mt-1 mb-0 flex items-center gap-1 text-caption text-fg-muted">
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
				{#if runState.progress && runState.progress.total > 1}
					<p class="mt-1 mb-0 font-mono text-caption tabular-nums text-fg-muted">
						Statement {runState.progress.index + 1} of {runState.progress.total}
					</p>
				{/if}
			</div>
		{:else if runState.status === 'ok'}
			{#if statements.length > 0}
				<div
					class="flex gap-1 overflow-x-auto border-b border-line bg-elevated px-2 py-1.5"
					role="tablist"
					aria-label="Result sets"
				>
					{#each statements as stmt, i (i)}
						<button
							type="button"
							role="tab"
							aria-selected={i === activeIndex}
							title={statementChipTitle(submittedText, stmt.sqlRange)}
							class={cn(
								'shrink-0 rounded-md px-2 py-0.5 font-mono text-caption tabular-nums',
								stmt.error
									? 'text-danger'
									: i === activeIndex
										? 'text-fg'
										: 'text-fg-muted',
								i === activeIndex ? 'bg-input' : 'hover:bg-input/70',
							)}
							onclick={() => onselectstatement?.(i)}
						>
							{statementChipLabel(statements, stmt)}
						</button>
					{/each}
				</div>
			{/if}
			{#if notices.length > 0}
				<div class="border-b border-line px-3 py-1.5">
					{#each notices as notice, i (i)}
						<p class="m-0 text-caption leading-relaxed text-fg-secondary">{notice}</p>
					{/each}
				</div>
			{/if}
			{#if activeStatement?.error}
				<div
					class="m-3 overflow-hidden rounded-lg border border-line bg-elevated shadow-sm"
					role="alert"
				>
					<div class="flex items-center gap-2 px-3 py-2">
						<p class="m-0 min-w-0 flex-1 truncate text-label font-semibold text-danger">
							Statement {statementChipLabel(statements, activeStatement)} failed
						</p>
						<IconButton
							label={copiedError ? 'Copied' : 'Copy error'}
							size="sm"
							variant="quiet"
							onclick={() => void copyError(activeStatement.error ?? '')}
						>
							<Icon name={copiedError ? 'check' : 'copy'} />
						</IconButton>
						{#if canLocate && onlocate}
							<Button size="sm" variant="secondary" onclick={onlocate}>
								Locate error
							</Button>
						{/if}
						{#if onaskai}
							<Button
								size="sm"
								variant="primary"
								onclick={() => onaskai(activeStatement.error ?? '')}
							>
								<Icon name="chat" class="size-3.5" />
								Ask AI
							</Button>
						{/if}
					</div>
					<pre
						class="m-0 max-h-40 overflow-auto border-t border-line bg-input px-3 py-2.5 font-mono text-label leading-relaxed text-fg select-text whitespace-pre-wrap">{activeStatement.error}</pre>
				</div>
			{:else if runState.result}
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
					{identStyle}
				/>
			{/if}
			{:else}
				<div
					class="flex flex-1 flex-col items-center justify-center px-6 py-8 text-center"
				>
					<p class="m-0 text-label font-medium text-fg">No tabular rows</p>
					<p class="mt-1 mb-0 max-w-prose-ui text-caption text-fg-muted">
						The statement finished without a result set.
					</p>
				</div>
			{/if}
		{:else if runState.status === 'blocked' || runState.status === 'error'}
			<div
				class="m-3 overflow-hidden rounded-lg border border-line bg-elevated shadow-sm"
				role="alert"
			>
				<div class="flex items-center gap-2 px-3 py-2">
					<p
						class={cn(
							'm-0 min-w-0 flex-1 truncate text-label font-semibold',
							runState.status === 'blocked' ? 'text-warning' : 'text-danger',
						)}
					>
						{runState.status === 'blocked' ? 'Query blocked' : 'Query failed'}
					</p>
					<IconButton
						label={copiedError ? 'Copied' : 'Copy error'}
						size="sm"
						variant="quiet"
						onclick={() => void copyError(runState.message)}
					>
						<Icon name={copiedError ? 'check' : 'copy'} />
					</IconButton>
					{#if canLocate && onlocate}
						<Button size="sm" variant="secondary" onclick={onlocate}>
							Locate error
						</Button>
					{/if}
					{#if onaskai}
						<Button
							size="sm"
							variant="primary"
							onclick={() => onaskai(runState.message)}
						>
							<Icon name="chat" class="size-3.5" />
							Ask AI
						</Button>
					{/if}
				</div>
				{#if errorParts.body}
					<pre
						class="m-0 max-h-40 overflow-auto border-t border-line bg-input px-3 py-2.5 font-mono text-label leading-relaxed text-fg select-text whitespace-pre-wrap">{errorParts.body}</pre>
				{/if}
				{#if errorParts.hint}
					<p
						class="m-0 border-t border-line px-3 py-2 text-caption leading-relaxed text-fg-secondary"
					>
						{errorParts.hint}
					</p>
				{/if}
			</div>
		{/if}
	</div>

	<StatusBar class="relative z-20" ariaLabel="Query results status">
		{#snippet left()}
			{#if hasRows}
				<span class="truncate font-mono tabular-nums">
					{resultPageStart + 1}–{resultPageEnd} of {resultTotal.toLocaleString()}
				</span>
			{:else if runState.status === 'ok' && resultColumns.length > 0}
				<span class="truncate font-mono tabular-nums">0 rows</span>
			{/if}
			{#if durationMs != null}
				<span title={`Ran in ${durationMs.toLocaleString()} ms`}>
					<Badge tone="neutral" size="sm">{formatDuration(durationMs)}</Badge>
				</span>
			{/if}
			{#if runState.status === 'ok' && runState.result?.truncated}
				<Badge tone="warning" size="sm">truncated</Badge>
			{/if}
			{#if runState.status === 'ok' && activeAffected != null && resultColumns.length > 0}
				<span class="truncate font-mono tabular-nums">
					{activeAffected.toLocaleString()} affected
				</span>
			{/if}
			{#if runState.status === 'ok' && runState.message}
				<span class="min-w-0 truncate text-fg-muted" title={runState.message}
					>{runState.message}</span
				>
			{/if}
			<StatusNote />
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
					placement="above-end"
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
		{/snippet}
	</StatusBar>
</div>
