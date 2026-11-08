<script lang="ts">
	/**
	 * Query space: Run → BE execute → results table.
	 * Composes toolbar / editor / results; native paste via bind:value.
	 */
	import type {
		CatalogColumnRow,
		EngineCapabilities,
		ExplainPlan,
		QueryMode,
	} from '#core/contracts'
	import {
		cancelTabRun,
		commitPinnedTab,
		executeEngineBatch,
		executePinnedBatch,
		explainEngineQuery,
		finishTabRun,
		isTauri,
		pinQueryTab,
		pinnedTabStatus,
		rollbackPinnedTab,
		startTabRun,
		unpinQueryTab,
		type PinStatus,
	} from '#core/ipc'
	import { Resizable } from '#core/ui'
	import { copyTextToClipboard } from '#core/util'
	import { connectionProfiles } from '#features/connections'
	import { showToast } from '#features/toast'
	import { catalogColumnsCache } from '#features/workspace/catalog'
	import {
		buildCsv,
		buildJson,
		exportFileBaseFromQuery,
		exportRowsAsCsv,
		exportRowsAsExcel,
		exportRowsAsJson,
	} from '#features/workspace/grid'
	import {
		isSessionMissingError,
		runWithWriteGuard,
		tabTxnView,
		workspaceSession,
	} from '#features/workspace/shell'
	import { appPrefs, resolveQueryTimeoutMs } from '#lib/app'
	import { onMount, untrack } from 'svelte'
	import { initialActiveIndex } from '../batch-view'
	import { QUERY_FETCH_MAX_ROWS, RESULT_PAGE_SIZE_DEFAULT } from '../constants'
	import { userFacingQueryError } from '../display'
	import type { QueryHistoryItem } from '../history-types'
	import { scanQueryParams, type ParamSlot } from '../params'
	import { queryLanguageFor, queryPlaceholder } from '../policy'
	import {
		editorEpoch,
		noteEditorEdit,
		placeDiagnostic,
		readQueryFault,
		type QueryDiagnostic,
	} from '../query-diagnostic'
	import type { RunState } from '../run-types'
	import {
		splitDialectFor,
		submissionStart,
		textToRun,
	} from '../statement-span'
	import ExplainView from './ExplainView.svelte'
	import QueryEditor from './QueryEditor.svelte'
	import QueryParamsDialog from './QueryParamsDialog.svelte'
	import { identStyleFor } from '#features/workspace/grid'
	import QueryResultsPanel, {
		type ResultExportAction,
	} from './QueryResultsPanel.svelte'

	interface Props {
		tabId: string
		connectionId: string
		connectionName: string
		engine: string
		profileReadOnly: boolean
		engineSessionId?: string | null
		database?: string | null
		text?: string
		dirty?: boolean
		saving?: boolean
		ontextchange?: (text: string) => void
		onhistory?: (item?: QueryHistoryItem | null) => void
		onsave?: () => void
		/** Re-open a dead engine session; returns the new session id. */
		onreconnect?: () => Promise<string | null>
		capabilities?: EngineCapabilities | null
	}

	let {
		tabId,
		connectionId,
		connectionName,
		engine,
		profileReadOnly,
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
	const resultPage = $derived(cachedRun?.resultPage ?? 0)
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
	const resultPageSize = RESULT_PAGE_SIZE_DEFAULT
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
		if (!enabled || !isTauri()) return
		void pinnedTabStatus(tab)
			.then((status) => applyPinStatus(tab, status))
			.catch(() => {})
	})

	$effect(() => {
		const sid = engineSessionId
		const db = database
		if (!isTauri() || !sid) return
		void catalogColumnsCache.load(sid, db || null)
	})
	const placeholder = $derived(queryPlaceholder(engine))
	const effectiveMode = $derived<QueryMode>(profileReadOnly ? 'read' : 'write')
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
		if (workspaceSession.getQueryRunCache(tabId)?.diagnostic) {
			persistRunCache(runState, resultPage, undefined, undefined, null)
		}
	})

	const batchResults = $derived(
		runState.status === 'ok' && runState.batch ? runState.batch.results : [],
	)
	const activeIndex = $derived(cachedRun?.activeIndex ?? 0)
	const resultRows = $derived(
		runState.status === 'ok' && runState.result ? runState.result.rows : [],
	)
	const resultColumns = $derived(
		runState.status === 'ok' && runState.result ? runState.result.columns : [],
	)
	const resultTotal = $derived(resultRows.length)
	const resultTotalPages = $derived(
		Math.max(1, Math.ceil(resultTotal / resultPageSize) || 1),
	)
	const resultSafePage = $derived(Math.min(resultPage, resultTotalPages - 1))
	const resultPageStart = $derived(resultSafePage * resultPageSize)
	const resultPageEnd = $derived(
		Math.min(resultPageStart + resultPageSize, resultTotal),
	)
	const resultPageRows = $derived(
		resultRows.slice(resultPageStart, resultPageEnd),
	)

	const canExport = $derived(
		runState.status === 'ok' &&
			!!runState.result &&
			runState.result.columns.length > 0 &&
			runState.result.rows.length > 0,
	)

	function tabStillOpen(): boolean {
		return workspaceSession.hasOpenTab(tabId)
	}

	function persistRunCache(
		state: RunState = runState,
		page = resultPage,
		submittedText?: string,
		activeIndex = cachedRun?.activeIndex ?? 0,
		diagnostic?: QueryDiagnostic | null,
		submittedStart?: number,
	) {
		if (!tabStillOpen()) return
		const prev = workspaceSession.getQueryRunCache(tabId)
		workspaceSession.setQueryRunCache(tabId, {
			runState: state,
			resultPage: page,
			submittedText: submittedText ?? prev?.submittedText,
			activeIndex,
			submittedStart: submittedStart ?? prev?.submittedStart,
			diagnostic: diagnostic === undefined ? prev?.diagnostic : diagnostic,
		})
	}

	function faultDiagnostic(
		message: string,
		position:
			| { offset: number; line: number; column: number }
			| null
			| undefined,
		start: number,
		startedEpoch: number,
	): QueryDiagnostic | null {
		if (!position || editorEpoch(tabId) !== startedEpoch) return null
		const doc = editorSnap.doc || currentQueryText()
		return placeDiagnostic(doc, start, position, message)
	}

	function selectStatement(index: number) {
		if (runState.status !== 'ok' || !runState.batch) return
		const stmt = runState.batch.results[index]
		if (!stmt) return
		persistRunCache(
			{
				...runState,
				message: stmt.error ? '' : (stmt.result?.message ?? '').trim(),
				result: stmt.result ?? null,
			},
			0,
			undefined,
			index,
		)
	}

	function emitText(value: string) {
		localText = value
		lastEmitted = value
		ontextchange?.(value)
		noteEditorEdit(tabId)
		if (workspaceSession.getQueryRunCache(tabId)?.diagnostic) {
			persistRunCache(runState, resultPage, undefined, undefined, null)
		}
	}

	function goResultPrev() {
		persistRunCache(runState, Math.max(0, resultSafePage - 1))
	}

	function goResultNext() {
		persistRunCache(
			runState,
			Math.min(resultTotalPages - 1, resultSafePage + 1),
		)
	}

	async function exportResults(action: ResultExportAction) {
		if (exporting) return
		if (runState.status !== 'ok' || !runState.result) return
		const result = runState.result
		if (result.columns.length === 0 || result.rows.length === 0) return
		exporting = action
		const cols = result.columns.map((c) => ({ name: c.name }))
		const rows = result.rows
		const fileBase = exportFileBaseFromQuery({
			queryText: currentQueryText(),
			language,
			connectionName,
			collection: language === 'mql' ? collection : null,
			database: language === 'mql' ? database : null,
		})
		const labels: Record<ResultExportAction, string> = {
			'download-csv': 'CSV downloaded',
			'download-json': 'JSON downloaded',
			'download-excel': 'Excel downloaded',
			'copy-csv': 'CSV copied',
			'copy-json': 'JSON copied',
		}
		const copy = action === 'copy-csv' || action === 'copy-json'
		try {
			let res
			switch (action) {
				case 'download-csv':
					res = await exportRowsAsCsv(cols, rows, fileBase)
					break
				case 'download-json':
					res = await exportRowsAsJson(cols, rows, fileBase)
					break
				case 'download-excel':
					res = await exportRowsAsExcel(cols, rows, fileBase)
					break
				case 'copy-csv':
					res = await copyTextToClipboard(buildCsv(cols, rows))
					break
				case 'copy-json':
					res = await copyTextToClipboard(buildJson(cols, rows, true))
					break
			}
			if (res.ok) showToast('success', labels[action])
			else if (res.cancelled) {
				showToast('info', copy ? 'Copy cancelled' : 'Export cancelled')
			} else {
				showToast(
					'danger',
					res.error || (copy ? 'Copy failed' : 'Export failed'),
				)
			}
		} catch (e) {
			showToast('danger', e instanceof Error ? e.message : String(e))
		} finally {
			exporting = null
		}
	}

	function currentQueryText(): string {
		return localText || text || ''
	}

	async function explainQuery() {
		if (runState.status === 'running') return
		const doc = editorSnap.doc || localText
		const dialect = splitDialectFor(engine)
		const sqlText = (
			textToRun(doc, dialect, editorSnap.head, editorSnap.anchor, false) ?? ''
		).trim()
		if (!sqlText) return
		const mode = effectiveMode
		const start = performance.now()
		explainPlan = null
		persistRunCache({ status: 'running' }, 0, sqlText, 0, null)

		if (!isTauri()) {
			persistRunCache({
				status: 'error',
				message:
					'Explain requires the Mystique desktop app. Connect and try again.',
				mode,
				elapsedMs: Math.round(performance.now() - start),
			})
			return
		}
		let sid = engineSessionId
		if (!sid && onreconnect) sid = await onreconnect()
		if (!sid) {
			persistRunCache({
				status: 'error',
				message:
					'No live engine session. Go Home → Connect so a pooled session is opened.',
				mode,
				elapsedMs: Math.round(performance.now() - start),
			})
			return
		}
		const executionId = startTabRun(tabId)
		try {
			const plan = await runWithWriteGuard(sid, (confirmObjectName) =>
				explainEngineQuery({
					sessionId: sid as string,
					text: sqlText,
					analyze,
					mode,
					timeoutMs,
					profileReadOnly,
					confirmObjectName: confirmObjectName ?? null,
					collection: language === 'mql' ? collection.trim() || null : null,
					database:
						language === 'mql' && database?.trim() ? database.trim() : null,
					executionId,
				}),
			)
			if (!tabStillOpen()) return
			explainPlan = plan
			persistRunCache({ status: 'idle' })
		} catch (e) {
			if (!tabStillOpen()) return
			const fault = readQueryFault(e)
			const msg = fault.message
			const elapsedMs = Math.round(performance.now() - start)
			const shown = /policy:/i.test(msg)
				? msg.replace(/^policy:\s*/i, '')
				: userFacingQueryError(msg)
			persistRunCache({
				status: /policy:/i.test(msg) ? 'blocked' : 'error',
				message: shown,
				mode,
				elapsedMs,
			})
		} finally {
			finishTabRun(tabId, executionId)
		}
	}

	async function runQuery(body?: string, origin = 0, bound?: unknown[]) {
		if (runState.status === 'running') return
		const mode = effectiveMode
		const sqlText = (body ?? currentQueryText()).trim()
		if (!sqlText) return
		if (bound === undefined && language !== 'mql') {
			const scanned = scanQueryParams(sqlText, splitDialectFor(engine))
			if (scanned.error) {
				persistRunCache({
					status: 'error',
					message: scanned.error,
					mode,
					elapsedMs: 0,
				})
				return
			}
			if (scanned.slots.length > 0) {
				paramPrompt = { text: sqlText, origin, slots: scanned.slots }
				return
			}
			bound = []
		}
		explainPlan = null
		const start = performance.now()
		const startedEpoch = editorEpoch(tabId)
		persistRunCache({ status: 'running' }, 0, sqlText, 0, null, origin)

		if (!isTauri()) {
			persistRunCache({
				status: 'error',
				message:
					'Live query execute requires the Mystique desktop app. Connect and try again.',
				mode,
				elapsedMs: Math.round(performance.now() - start),
			})
			return
		}

		let sid = engineSessionId
		if (!sid && onreconnect) {
			sid = await onreconnect()
		}
		if (!sid) {
			const elapsedMs = Math.round(performance.now() - start)
			persistRunCache({
				status: 'error',
				message:
					'No live engine session. Go Home → Connect so a pooled session is opened.',
				mode,
				elapsedMs,
			})
			return
		}

		const executionId = startTabRun(tabId)
		try {
			const request = {
				sessionId: sid,
				text: sqlText,
				mode,
				maxRows: QUERY_FETCH_MAX_ROWS,
				timeoutMs,
				profileReadOnly,
				collection: language === 'mql' ? collection.trim() || null : null,
				database: database?.trim() ? database.trim() : null,
				executionId,
				params: bound ?? [],
			}
			const onProgress = (event: { index: number; total: number }) => {
				if (!tabStillOpen()) return
				const current = workspaceSession.getQueryRunCache(tabId)
				if (!current || current.runState.status !== 'running') return
				persistRunCache(
					{
						status: 'running',
						progress: { index: event.index, total: event.total },
					},
					current.resultPage,
					current.submittedText,
					current.activeIndex ?? 0,
				)
			}
			const runBatch = (
				sessionId: string,
				confirmObjectName: string | null,
			) => {
				const body = {
					...request,
					sessionId,
					confirmObjectName,
				}
				return manualCommit
					? executePinnedBatch(body, tabId, onProgress)
					: executeEngineBatch(body, onProgress)
			}
			let batch
			try {
				batch = await runWithWriteGuard(sid, (confirmObjectName) =>
					runBatch(sid, confirmObjectName ?? null),
				)
			} catch (e) {
				// Read mode rolls back. A write may already have landed.
				if (mode === 'write' || !onreconnect || !isSessionMissingError(e))
					throw e
				const next = await onreconnect()
				if (!next) throw e
				batch = await runWithWriteGuard(next, (confirmObjectName) =>
					runBatch(next, confirmObjectName ?? null),
				)
			}
			const elapsedMs = Math.round(performance.now() - start)
			if (!tabStillOpen()) return
			const index = initialActiveIndex(batch.results, batch.stoppedAt)
			const active = batch.results[index]
			const diagnostic = active?.error
				? faultDiagnostic(
						active.error,
						active.errorPosition,
						origin,
						startedEpoch,
					)
				: null
			persistRunCache(
				{
					status: 'ok',
					message: active?.error ? '' : (active?.result?.message ?? '').trim(),
					mode,
					elapsedMs,
					result: active?.result ?? null,
					batch,
				},
				0,
				sqlText,
				index,
				diagnostic,
				origin,
			)
			onhistory?.(null)
		} catch (e) {
			if (!tabStillOpen()) return
			const fault = readQueryFault(e)
			const msg = fault.message
			const isPolicy = /policy:/i.test(msg) || /blocked/i.test(msg)
			const elapsedMs = Math.round(performance.now() - start)
			const shown = isPolicy
				? msg.replace(/^policy:\s*/i, '')
				: userFacingQueryError(msg)
			persistRunCache(
				{
					status: isPolicy ? 'blocked' : 'error',
					message: shown,
					mode,
					elapsedMs,
				},
				0,
				sqlText,
				fault.statementIndex ?? 0,
				faultDiagnostic(shown, fault.position, origin, startedEpoch),
				origin,
			)
			onhistory?.(null)
		} finally {
			finishTabRun(tabId, executionId)
			if (manualCommit) void refreshPin()
		}
	}

	function cancelRun() {
		if (runState.status !== 'running') return
		void cancelTabRun(tabId)
	}

	async function refreshPin() {
		if (!showTransactions || !isTauri()) return
		try {
			const status = await pinnedTabStatus(tabId)
			applyPinStatus(tabId, status)
		} catch {
			/* The toolbar keeps the last status. */
		}
	}

	/** Tabs already told that the backend dropped their transaction. */
	const rollbackNoticed = new Set<string>()

	function applyPinStatus(tab: string, status: PinStatus) {
		workspaceSession.setTabTxn(tab, status)
		if (!status.rolledBack) {
			rollbackNoticed.delete(tab)
			return
		}
		if (rollbackNoticed.has(tab)) return
		rollbackNoticed.add(tab)
		showToast(
			'warning',
			'The open transaction was rolled back (idle timeout or lost connection). Nothing in it was committed.',
		)
	}

	async function setManualCommit(manual: boolean) {
		if (!engineSessionId) {
			showToast('danger', 'Connect before changing auto-commit.')
			return
		}
		const tab = tabId
		try {
			const status = manual
				? await pinQueryTab(engineSessionId, tab)
				: await unpinQueryTab(tab)
			workspaceSession.setTabTxn(tab, {
				pinned: manual ? status.pinned : false,
				inTransaction: status.inTransaction,
			})
			if (!manual && status.rolledBack) showToast('info', 'Open transaction rolled back.')
		} catch (e) {
			showToast('danger', e instanceof Error ? e.message : String(e))
		}
	}

	async function commitPin() {
		try {
			const status = await commitPinnedTab(tabId)
			workspaceSession.setTabTxn(tabId, status)
			showToast('success', 'Committed.')
		} catch (e) {
			showToast('danger', e instanceof Error ? e.message : String(e))
		}
	}

	async function rollbackPin() {
		try {
			const status = await rollbackPinnedTab(tabId)
			workspaceSession.setTabTxn(tabId, status)
			showToast('info', 'Rolled back.')
		} catch (e) {
			showToast('danger', e instanceof Error ? e.message : String(e))
		}
	}

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

	onMount(() => {
		const onKey = (e: KeyboardEvent) => {
			if (e.key === 'Escape' && runState.status === 'running') {
				const t = e.target
				if (t instanceof HTMLElement) {
					if (t.closest('[role="dialog"]')) return
					if (t.closest('.cm-mystique-search')) return
				}
				e.preventDefault()
				e.stopPropagation()
				cancelRun()
				return
			}
			if (!(e.metaKey || e.ctrlKey) || e.altKey) return
			if (e.key !== 'Enter' && e.code !== 'NumpadEnter') return
			const t = e.target
			if (t instanceof HTMLElement) {
				if (t.closest('[role="dialog"]')) return
				// The editor keymap owns this chord so it can see the caret.
				if (t.closest('.cm-editor')) return
				if (t.closest('textarea')) return
			}
			e.preventDefault()
			e.stopImmediatePropagation()
			const doc = editorSnap.doc || currentQueryText()
			const dialect = splitDialectFor(engine)
			const text = textToRun(
				doc,
				dialect,
				editorSnap.head,
				editorSnap.anchor,
				e.shiftKey,
			)
			if (!text) return
			void runQuery(
				text,
				submissionStart(
					doc,
					dialect,
					editorSnap.head,
					editorSnap.anchor,
					e.shiftKey,
				),
			)
		}
		window.addEventListener('keydown', onKey, true)
		return () => window.removeEventListener('keydown', onKey, true)
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
					{resultColumns}
					{resultPageRows}
					{resultTotal}
					{resultPageStart}
					{resultPageEnd}
					{resultSafePage}
					{resultTotalPages}
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
