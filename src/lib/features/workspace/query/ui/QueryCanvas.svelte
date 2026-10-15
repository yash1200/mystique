<script lang="ts">
	/**
	 * Query space: Run → BE execute → results table.
	 * Composes toolbar / editor / results; native paste via bind:value.
	 */
	import type { EngineCapabilities, QueryMode } from '#core/contracts'
	import {
		cancelTabRun,
		commitPinnedTab,
		executeEngineBatch,
		executePinnedBatch,
		pinQueryTab,
		pinnedTabStatus,
		rollbackPinnedTab,
		unpinQueryTab,
		finishTabRun,
		isTauri,
		startTabRun,
	} from '#core/ipc'
	import { Resizable } from '#core/ui'
	import { copyTextToClipboard } from '#core/util'
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
		workspaceSession,
	} from '#features/workspace/shell'
	import { appPrefs } from '#lib/app'
	import { onMount, untrack } from 'svelte'
	import { initialActiveIndex } from '../batch-view'
	import {
		editorEpoch,
		noteEditorEdit,
		placeDiagnostic,
		readQueryFault,
		type QueryDiagnostic,
	} from '../query-diagnostic'
	import { splitDialectFor, submissionStart, textToRun } from '../statement-span'
	import { QUERY_FETCH_MAX_ROWS, RESULT_PAGE_SIZE_DEFAULT } from '../constants'
	import { userFacingQueryError } from '../display'
	import { appendQueryHistory } from '../history-api'
	import type { QueryHistoryItem } from '../history-types'
	import { queryLanguageFor, queryPlaceholder } from '../policy'
	import type { RunState } from '../run-types'
	import QueryEditor from './QueryEditor.svelte'
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
	const catalogColumns = $derived(
		catalogColKey ? (catalogColumnsCache.byKey[catalogColKey] ?? []) : [],
	)
	const resultPageSize = RESULT_PAGE_SIZE_DEFAULT
	let editorSnap = $state({ doc: '', head: 0, anchor: 0 })
	let exporting = $state<ResultExportAction | null>(null)
	let locateNonce = $state(0)
	let manualCommit = $state(false)
	let transactionOpen = $state(false)
	const showTransactions = $derived(capabilities?.supportsTransactions === true)

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
			.then((status) => {
				if (tab !== tabId) return
				manualCommit = status.pinned
				transactionOpen = status.inTransaction
			})
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

	const lineCount = $derived.by(() => {
		if (!localText) return 1
		return localText.split('\n').length
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
		position: { offset: number; line: number; column: number } | null | undefined,
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
				showToast('danger', res.error || (copy ? 'Copy failed' : 'Export failed'))
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

	async function recordHistory(opts: {
		success: boolean
		durationMs: number
		rowCount?: number | null
		errorMessage?: string | null
		body?: string
	}): Promise<QueryHistoryItem | null> {
		if (!isTauri()) return null
		const body = (opts.body ?? currentQueryText()).trim()
		if (!body) return null
		try {
			const item = await appendQueryHistory({
				connectionId,
				engine,
				body,
				language: language === 'mql' ? 'mql' : 'sql',
				success: opts.success,
				rowCount: opts.rowCount ?? null,
				durationMs: opts.durationMs,
				errorMessage: opts.errorMessage ?? null,
			})
			onhistory?.(item)
			return item
		} catch (e) {
			const msg = e instanceof Error ? e.message : String(e)
			console.warn('[mystique] failed to append query history:', msg)
			onhistory?.(null)
			return null
		}
	}

	async function runQuery(body?: string, origin = 0) {
		if (runState.status === 'running') return
		const mode = effectiveMode
		const start = performance.now()
		const sqlText = (body ?? currentQueryText()).trim()
		if (!sqlText) return
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
			await recordHistory({
				success: false,
				durationMs: elapsedMs,
				errorMessage: 'No live engine session',
				body: sqlText,
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
				timeoutMs: 120_000,
				profileReadOnly,
				collection: language === 'mql' ? collection.trim() || null : null,
				database:
					language === 'mql' && database?.trim() ? database.trim() : null,
				executionId,
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
				if (mode === 'write' || !onreconnect || !isSessionMissingError(e)) throw e
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
			const diagnostic =
				active?.error
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
			const failed = batch.stoppedAt != null
			await recordHistory({
				success: !failed,
				durationMs: active?.result?.durationMs ?? elapsedMs,
				rowCount: active?.result?.rowCount,
				errorMessage: failed ? (active?.error ?? 'Statement failed.') : null,
				body: sqlText,
			})
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
			await recordHistory({
				success: false,
				durationMs: elapsedMs,
				errorMessage: msg,
				body: sqlText,
			})
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
			manualCommit = status.pinned
			transactionOpen = status.inTransaction
		} catch {
			/* The toolbar keeps the last status. */
		}
	}

	async function setManualCommit(manual: boolean) {
		if (!engineSessionId) {
			showToast('danger', 'Connect before changing auto-commit.')
			return
		}
		const wasOpen = transactionOpen
		try {
			const status = manual
				? await pinQueryTab(engineSessionId, tabId)
				: await unpinQueryTab(tabId)
			manualCommit = manual ? status.pinned : false
			transactionOpen = status.inTransaction
			if (!manual && wasOpen) showToast('info', 'Open transaction rolled back.')
		} catch (e) {
			showToast('danger', e instanceof Error ? e.message : String(e))
		}
	}

	async function commitPin() {
		try {
			const status = await commitPinnedTab(tabId)
			transactionOpen = status.inTransaction
			showToast('success', 'Committed.')
		} catch (e) {
			showToast('danger', e instanceof Error ? e.message : String(e))
		}
	}

	async function rollbackPin() {
		try {
			const status = await rollbackPinnedTab(tabId)
			transactionOpen = status.inTransaction
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
				{connectionId}
				{engine}
				{placeholder}
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
			<QueryResultsPanel
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
		</div>
	</div>
</div>
