<script lang="ts">
	/**
	 * Query space: Run → BE execute → results table.
	 * Composes editor / results / explain / params. Runs, cancel and the
	 * pinned session live in `../run/run.svelte`, keyed by tab.
	 */
	import type {
		CatalogColumnRow,
		EngineCapabilities,
		ExplainPlan,
		QueryMode,
	} from '#core/contracts'
	import { pinnedTabStatus } from '#core/ipc'
	import { commands, type CommandContext } from '#core/commands'
	import { Resizable } from '#core/ui'
	import { connectionProfiles } from '#features/connections'
	import { catalogColumnsCache } from '#features/workspace/catalog'
	import { identStyleFor } from '#features/workspace/data-grid'
	import { tabTxnView, workspaceSession } from '#features/workspace/session'
	import { appPrefs, cmd, resolveQueryTimeoutMs } from '#lib/app'
	import { onMount, untrack } from 'svelte'
	import { RESULT_PAGE_SIZE_DEFAULT } from '../constants'
	import QueryEditor from '../editor/QueryEditor.svelte'
	import { noteEditorEdit } from '../editor/query-diagnostic'
	import {
		splitDialectFor,
		submissionStart,
		textToRun,
	} from '../editor/statement-span'
	import { runExplain } from '../explain/explain'
	import ExplainView from '../explain/ExplainView.svelte'
	import type { QueryHistoryItem } from '../history/history-types'
	import type { ParamSlot } from '../params/params'
	import QueryParamsDialog from '../params/QueryParamsDialog.svelte'
	import { queryLanguageFor, queryPlaceholder } from '../policy'
	import { canExportResult, exportResult } from '../results/export-results'
	import { resultPage } from '../results/paging'
	import QueryResultsPanel, {
		type ResultExportAction,
	} from '../results/QueryResultsPanel.svelte'
	import {
		applyPinStatus,
		cancelRun as cancelTabQuery,
		clearDiagnostic,
		commitPin as commitTabPin,
		rollbackPin as rollbackTabPin,
		runQuery as runTabQuery,
		selectStatement as selectTabStatement,
		setManualCommit as setTabManualCommit,
		setResultPage,
		type RunTarget,
	} from '../run/run.svelte'
	import type { RunState } from '../run/run-types'

	interface Props {
		tabId: string
		connectionId: string
		connectionName: string
		engine: string
		readOnly: boolean
		engineSessionId?: string | null
		database?: string | null
		text?: string
		dirty?: boolean
		saving?: boolean
		ontextchange?: (text: string) => void
		onhistory?: (item?: QueryHistoryItem | null) => void
		onsave?: () => void
		/** Re-open a dead engine session; returns the new session id. */
		/** Reconnects the given connection; a run passes the one it started on. */
		onreconnect?: (connectionId: string) => Promise<string | null>
		capabilities?: EngineCapabilities | null
	}

	let {
		tabId,
		connectionId,
		connectionName,
		engine,
		readOnly,
		engineSessionId = null,
		database = null,
		text = '',
		dirty = false,
		saving = false,
		ontextchange,
		onhistory,
		onsave,
		onreconnect,
		capabilities = null,
	}: Props = $props()

	// The store holds the run, including `running`, so unmounting this canvas
	// does not drop it or start a second one. Rows stay unproxied in the Map.
	const cachedRun = $derived(workspaceSession.getQueryRunCache(tabId))
	const runState = $derived<RunState>(cachedRun?.runState ?? { status: 'idle' })
	let collection = $state('')
	const catalogColKey = $derived(
		engineSessionId
			? catalogColumnsCache.key(engineSessionId, database || null)
			: '',
	)
	const noColumns: CatalogColumnRow[] = []
	const noSlots: ParamSlot[] = []
	const catalogColumns = $derived(
		catalogColKey
			? (catalogColumnsCache.byKey[catalogColKey] ?? noColumns)
			: noColumns,
	)
	let editorSnap = $state({ doc: '', head: 0, anchor: 0 })
	let exporting = $state<ResultExportAction | null>(null)
	let locateNonce = $state(0)
	// Per-tab, from the store: correct on the first frame after a tab switch.
	const txn = $derived(tabTxnView(workspaceSession.tabTxn, tabId))
	const manualCommit = $derived(txn.manualCommit)
	const transactionOpen = $derived(txn.transactionOpen)
	let analyze = $state(false)
	let explainPlan = $state<ExplainPlan | null>(null)
	let paramPrompt = $state<{ text: string; origin: number; slots: ParamSlot[] } | null>(
		null,
	)
	let canvasTab = ''
	const showTransactions = $derived(capabilities?.supportsTransactions === true)
	const showExplain = $derived(capabilities?.supportsExplainPlan === true)

	// One-time seed from prop; $effect below re-syncs when parent draft changes.
	let localText = $state(untrack(() => text ?? ''))
	let lastEmitted = $state(untrack(() => text ?? ''))

	/** Vertical split: editor pane share (results get the rest). */
	const SPLIT_MIN = 18
	const SPLIT_MAX = 82
	const SPLIT_DEFAULT = 42
	let editorSharePct = $state(SPLIT_DEFAULT)
	let splitRootEl = $state<HTMLDivElement | null>(null)
	let splitDragging = $state(false)

	const language = $derived(queryLanguageFor(engine, capabilities))

	$effect(() => {
		const tab = tabId
		const enabled = showTransactions
		if (!enabled) return
		void pinnedTabStatus(tab)
			.then((status) => applyPinStatus(tab, status))
			.catch(() => {})
	})

	$effect(() => {
		const sid = engineSessionId
		const db = database
		if (!sid) return
		void catalogColumnsCache.load(sid, db || null)
	})
	const placeholder = $derived(queryPlaceholder(engine))
	const effectiveMode = $derived<QueryMode>(readOnly ? 'read' : 'write')
	// Profile, then the app setting, then 120 s. 0 is sent as "no timeout".
	const timeoutMs = $derived(
		resolveQueryTimeoutMs(
			connectionProfiles.profile(connectionId)?.queryTimeoutSecs,
			appPrefs.queryTimeoutSecs,
		),
	)

	const lineCount = $derived.by(() => {
		if (!localText) return 1
		return localText.split('\n').length
	})

	// One canvas instance covers every query tab. Reset the bits `{#key}` used to drop.
	$effect(() => {
		const id = tabId
		if (!canvasTab) {
			canvasTab = id
			return
		}
		if (id === canvasTab) return
		canvasTab = id
		explainPlan = null
		analyze = false
		collection = ''
		paramPrompt = null
		editorSharePct = SPLIT_DEFAULT
		const next = text ?? ''
		localText = next
		lastEmitted = next
		editorSnap = { doc: next, head: 0, anchor: 0 }
	})

	// Sync parent draft (e.g. open from history) when not typing in the editor.
	$effect(() => {
		const prop = text ?? ''
		if (prop === lastEmitted || prop === localText) return
		if (typeof document !== 'undefined') {
			const ae = document.activeElement
			if (ae?.closest?.(`[data-query-editor="${connectionId}"]`)) {
				return
			}
		}
		noteEditorEdit(tabId)
		localText = prop
		lastEmitted = prop
		clearDiagnostic(tabId)
	})

	const batchResults = $derived(
		runState.status === 'ok' && runState.batch ? runState.batch.results : [],
	)
	const activeIndex = $derived(cachedRun?.activeIndex ?? 0)
	const result = $derived(
		runState.status === 'ok' ? (runState.result ?? null) : null,
	)
	const page = $derived(
		resultPage(result?.rows ?? [], cachedRun?.resultPage ?? 0, RESULT_PAGE_SIZE_DEFAULT),
	)
	const canExport = $derived(canExportResult(result))

	/**
	 * This tab's run settings, read now. A run keeps them to the end, so it
	 * reports to its own tab even when the canvas has moved on to another.
	 */
	function runTarget(): RunTarget {
		const runConnection = connectionId
		return {
			tabId,
			engine,
			language,
			mode: effectiveMode,
			timeoutMs,
			sessionId: engineSessionId,
			database,
			collection,
			manualCommit,
			transactions: showTransactions,
			reconnect: onreconnect ? () => onreconnect(runConnection) : undefined,
			onhistory,
			onstart: () => (explainPlan = null),
			doc: () => editorSnap.doc || currentQueryText(),
		}
	}

	function currentQueryText(): string {
		return localText || text || ''
	}

	function emitText(value: string) {
		localText = value
		lastEmitted = value
		ontextchange?.(value)
		noteEditorEdit(tabId)
		clearDiagnostic(tabId)
	}

	async function runQuery(body?: string, origin = 0, bound?: unknown[]) {
		const outcome = await runTabQuery(
			runTarget(),
			body ?? currentQueryText(),
			origin,
			bound,
		)
		if (outcome.kind === 'params') {
			paramPrompt = { text: outcome.text, origin: outcome.origin, slots: outcome.slots }
		}
	}

	async function explainQuery() {
		if (runState.status === 'running') return
		const doc = editorSnap.doc || localText
		const sqlText = textToRun(
			doc,
			splitDialectFor(engine),
			editorSnap.head,
			editorSnap.anchor,
			false,
		)
		if (!sqlText?.trim()) return
		const started = tabId
		explainPlan = null
		const plan = await runExplain(runTarget(), sqlText, analyze)
		// The canvas may show another tab by now; that tab gets no plan.
		if (plan && tabId === started) explainPlan = plan
	}

	async function exportResults(action: ResultExportAction) {
		if (exporting || !result || !canExportResult(result)) return
		exporting = action
		try {
			await exportResult(action, result, {
				queryText: currentQueryText(),
				language,
				connectionName,
				collection: language === 'mql' ? collection : null,
				database: language === 'mql' ? database : null,
			})
		} finally {
			exporting = null
		}
	}

	const cancelRun = () => cancelTabQuery(tabId)
	const selectStatement = (index: number) => selectTabStatement(tabId, index)
	const goResultPrev = () => setResultPage(tabId, Math.max(0, page.page - 1))
	const goResultNext = () =>
		setResultPage(tabId, Math.min(page.totalPages - 1, page.page + 1))
	const setManualCommit = (manual: boolean) =>
		setTabManualCommit(tabId, engineSessionId, manual)
	const commitPin = () => commitTabPin(tabId, showTransactions)
	const rollbackPin = () => rollbackTabPin(tabId, showTransactions)

	function askAiAboutError(errorMessage: string) {
		const error = errorMessage.trim()
		const diag = workspaceSession.getQueryRunCache(tabId)?.diagnostic
		const where = diag
			? `The failing token is at line ${diag.line}, column ${diag.column}.\n\n`
			: ''
		workspaceSession.queueAgentPrompt(
			error
				? `The query in the editor failed with this error. Debug it and fix it.\n\n${where}${error}`
				: 'The query in the editor failed. Debug it and fix it.',
		)
		appPrefs.setAgentChatOpen(true)
	}

	function locateError() {
		if (!workspaceSession.getQueryRunCache(tabId)?.diagnostic) return
		locateNonce += 1
	}

	function clampSplit(pct: number): number {
		return Math.min(SPLIT_MAX, Math.max(SPLIT_MIN, Math.round(pct * 10) / 10))
	}

	/** ⌘Enter / ⌘⇧Enter with focus in the tab but outside the editor. */
	function runFromKeys(all: boolean) {
		const doc = editorSnap.doc || currentQueryText()
		const dialect = splitDialectFor(engine)
		const text = textToRun(doc, dialect, editorSnap.head, editorSnap.anchor, all)
		if (!text) return
		void runQuery(
			text,
			submissionStart(doc, dialect, editorSnap.head, editorSnap.anchor, all),
		)
	}

	onMount(() => {
		// The editor keymap owns ⌘Enter inside the editor so it can see the caret.
		const notInTextarea = ({ focus }: CommandContext) =>
			!focus?.target?.closest('textarea')
		return commands.register([
			{
				...cmd.cancelRun,
				when: ({ focus }) =>
					runState.status === 'running' &&
					!focus?.target?.closest('.cm-mystique-search'),
				run: () => cancelRun(),
			},
			{ ...cmd.run, when: notInTextarea, run: () => runFromKeys(false) },
			{ ...cmd.runAll, when: notInTextarea, run: () => runFromKeys(true) },
		])
	})
</script>

<div
	class="flex min-h-0 min-w-0 flex-1 flex-col bg-app-wash"
	data-connection-id={connectionId}
>
	<div
		bind:this={splitRootEl}
		class="flex min-h-0 flex-1 flex-col"
		class:select-none={splitDragging}
	>
		<div
			class="flex min-h-0 min-w-0 flex-col overflow-hidden bg-elevated"
			style="flex: 0 0 {editorSharePct}%; max-height: {SPLIT_MAX}%"
		>
			<QueryEditor
				{tabId}
				{connectionId}
				{engine}
				{placeholder}
				draft={text ?? ''}
				bind:localText
				{lineCount}
				{language}
				{catalogColumns}
				bind:collection
				canRun={!!localText.trim()}
				canCancel={capabilities?.supportsCancelQuery === true}
				showStatementGutter={capabilities?.supportsQuery !== false}
				running={runState.status === 'running'}
				oncancel={cancelRun}
				{showExplain}
				{analyze}
				onanalyze={(on) => (analyze = on)}
				onexplain={() => void explainQuery()}
				{showTransactions}
				{manualCommit}
				{transactionOpen}
				onmanualcommit={(manual) => void setManualCommit(manual)}
				oncommit={() => void commitPin()}
				onrollback={() => void rollbackPin()}
				{dirty}
				{saving}
				onchange={emitText}
				onactivity={(snap) => (editorSnap = snap)}
				diagnostic={cachedRun?.diagnostic ?? null}
				{locateNonce}
				onrun={(request) => void runQuery(request.text, request.start)}
				{onsave}
			/>
		</div>

		<Resizable
			axis="y"
			variant="split"
			size={editorSharePct}
			defaultSize={SPLIT_DEFAULT}
			min={SPLIT_MIN}
			max={SPLIT_MAX}
			step={2}
			track={splitRootEl}
			label="Resize editor and results"
			bind:dragging={splitDragging}
			onresize={(n) => (editorSharePct = clampSplit(n))}
		/>

		<div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
			{#if explainPlan}
				<ExplainView plan={explainPlan} />
			{:else}
				<QueryResultsPanel
					identStyle={identStyleFor(engine, capabilities?.sqlDialect ?? '')}
					{runState}
					resultColumns={result?.columns ?? []}
					resultPageRows={page.rows}
					resultTotal={page.total}
					resultPageStart={page.start}
					resultPageEnd={page.end}
					resultSafePage={page.page}
					resultTotalPages={page.totalPages}
					{canExport}
					{exporting}
					statements={batchResults}
					{activeIndex}
					submittedText={cachedRun?.submittedText ?? ''}
					onselectstatement={selectStatement}
					refreshing={runState.status === 'running'}
					onexport={(f) => void exportResults(f)}
					onprev={goResultPrev}
					onnext={goResultNext}
					canLocate={!!cachedRun?.diagnostic}
					onlocate={locateError}
					onrefresh={() => {
						const cached = workspaceSession.getQueryRunCache(tabId)
						void runQuery(cached?.submittedText, cached?.submittedStart ?? 0)
					}}
					onaskai={askAiAboutError}
				/>
			{/if}
		</div>
	</div>
	<QueryParamsDialog
		open={paramPrompt != null}
		slots={paramPrompt?.slots ?? noSlots}
		oncancel={() => {
			paramPrompt = null
		}}
		onconfirm={(values) => {
			const pending = paramPrompt
			paramPrompt = null
			if (!pending) return
			void runQuery(pending.text, pending.origin, values)
		}}
	/>
</div>
