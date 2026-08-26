<script lang="ts">
	/**
	 * Query space: Run → BE execute → results table.
	 * Composes toolbar / editor / results; native paste via bind:value.
	 */
	import {
		executeEngineQuery,
		listEngineCatalogColumns,
		type CatalogColumnRow,
	} from '$lib/api/engines'
	import { appendQueryHistory, type QueryHistoryItem } from '$lib/api/history'
	import { isTauri } from '$lib/api/tauri'
	import { Resizable } from '$lib/components/core'
	import { workspaceSession } from '$lib/stores/workspace-session.svelte'
	import type { EngineCapabilities } from '$lib/types/engineCaps'
	import {
		buildCsv,
		buildJson,
		copyTextToClipboard,
		exportFileBaseFromQuery,
		exportRowsAsCsv,
		exportRowsAsExcel,
		exportRowsAsJson,
	} from '$lib/utils/exportTable'
	import {
		queryLanguageFor,
		queryPlaceholder,
		type QueryMode,
	} from '$lib/utils/queryPolicyFe'
	import { onMount, untrack } from 'svelte'
	import QueryEditor from './query/QueryEditor.svelte'
	import QueryResultsPanel, {
		type ResultExportAction,
	} from './query/QueryResultsPanel.svelte'
	import {
		QUERY_FETCH_MAX_ROWS,
		RESULT_PAGE_SIZE_DEFAULT,
		type RunState,
	} from './query/types'
	import { isSessionMissingError } from './shell/sessionUtils'

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

	const cached = untrack(() => workspaceSession.getQueryRunCache(tabId))
	// Raw: result grids can be tens of thousands of rows. Deep $state
	// proxies on that payload freeze the UI after Run.
	let runState = $state.raw<RunState>(cached?.runState ?? { status: 'idle' })
	let collection = $state('')
	let catalogColumns = $state<CatalogColumnRow[]>([])
	let resultPage = $state(cached?.resultPage ?? 0)
	const resultPageSize = RESULT_PAGE_SIZE_DEFAULT
	let exporting = $state<ResultExportAction | null>(null)
	let exportMsg = $state<string | null>(null)

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
		const sid = engineSessionId
		const db = database
		if (!isTauri() || !sid) {
			catalogColumns = []
			return
		}
		let cancelled = false
		void listEngineCatalogColumns({
			sessionId: sid,
			database: db || null,
		})
			.then((rows) => {
				if (!cancelled) catalogColumns = rows ?? []
			})
			.catch(() => {
				if (!cancelled) catalogColumns = []
			})
		return () => {
			cancelled = true
		}
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
		localText = prop
		lastEmitted = prop
	})

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

	function persistRunCache(state: RunState = runState, page = resultPage) {
		if (state.status === 'running') return
		workspaceSession.setQueryRunCache(tabId, {
			runState: state,
			resultPage: page,
		})
	}

	function emitText(value: string) {
		localText = value
		lastEmitted = value
		ontextchange?.(value)
	}

	function goResultPrev() {
		resultPage = Math.max(0, resultSafePage - 1)
		persistRunCache(runState, resultPage)
	}

	function goResultNext() {
		resultPage = Math.min(resultTotalPages - 1, resultSafePage + 1)
		persistRunCache(runState, resultPage)
	}

	async function exportResults(action: ResultExportAction) {
		if (exporting) return
		if (runState.status !== 'ok' || !runState.result) return
		const result = runState.result
		if (result.columns.length === 0 || result.rows.length === 0) return
		exporting = action
		exportMsg = null
		const cols = result.columns.map((c) => ({ name: c.name }))
		const rows = result.rows
		const fileBase = exportFileBaseFromQuery({
			queryText: currentQueryText(),
			language,
			connectionName,
			collection: language === 'mql' ? collection : null,
			database: language === 'mql' ? database : null,
		})
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
			if (res.ok) {
				const labels: Record<ResultExportAction, string> = {
					'download-csv': 'CSV downloaded',
					'download-json': 'JSON downloaded',
					'download-excel': 'Excel downloaded',
					'copy-csv': 'CSV copied',
					'copy-json': 'JSON copied',
				}
				exportMsg = labels[action]
			} else if (!res.cancelled) {
				exportMsg = res.error
			}
		} catch (e) {
			exportMsg = e instanceof Error ? e.message : String(e)
		} finally {
			exporting = null
			setTimeout(() => {
				if (
					exportMsg &&
					!exportMsg.toLowerCase().includes('fail') &&
					!exportMsg.toLowerCase().includes('error')
				) {
					exportMsg = null
				}
			}, 2500)
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

	async function runQuery() {
		if (runState.status === 'running') return
		const mode = effectiveMode
		const start = performance.now()
		resultPage = 0
		exportMsg = null
		runState = { status: 'running' }
		const sqlText = currentQueryText()

		if (!isTauri()) {
			runState = {
				status: 'error',
				message:
					'Live query execute requires Tauri. Run `npm run tauri dev` and Connect again.',
				mode,
				elapsedMs: Math.round(performance.now() - start),
			}
			persistRunCache()
			return
		}

		let sid = engineSessionId
		if (!sid && onreconnect) {
			sid = await onreconnect()
		}
		if (!sid) {
			const elapsedMs = Math.round(performance.now() - start)
			runState = {
				status: 'error',
				message:
					'No live engine session. Go Home → Connect so a pooled session is opened.',
				mode,
				elapsedMs,
			}
			persistRunCache()
			await recordHistory({
				success: false,
				durationMs: elapsedMs,
				errorMessage: 'No live engine session',
				body: sqlText,
			})
			return
		}

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
			}
			let result
			try {
				result = await executeEngineQuery(request)
			} catch (e) {
				if (!onreconnect || !isSessionMissingError(e)) throw e
				const next = await onreconnect()
				if (!next) throw e
				result = await executeEngineQuery({ ...request, sessionId: next })
			}
			const elapsedMs = Math.round(performance.now() - start)
			runState = {
				status: 'ok',
				message: (result.message ?? '').trim(),
				mode,
				elapsedMs,
				result,
			}
			persistRunCache()
			await recordHistory({
				success: true,
				durationMs: result.durationMs ?? elapsedMs,
				rowCount: result.rowCount,
				body: sqlText,
			})
		} catch (e) {
			const msg = e instanceof Error ? e.message : String(e)
			const isPolicy = /policy:/i.test(msg) || /blocked/i.test(msg)
			const elapsedMs = Math.round(performance.now() - start)
			runState = {
				status: isPolicy ? 'blocked' : 'error',
				message: msg.replace(/^policy:\s*/i, ''),
				mode,
				elapsedMs,
			}
			persistRunCache()
			await recordHistory({
				success: false,
				durationMs: elapsedMs,
				errorMessage: msg,
				body: sqlText,
			})
		}
	}

	function clampSplit(pct: number): number {
		return Math.min(SPLIT_MAX, Math.max(SPLIT_MIN, Math.round(pct * 10) / 10))
	}

	onMount(() => {
		const onKey = (e: KeyboardEvent) => {
			if (!(e.metaKey || e.ctrlKey) || e.shiftKey || e.altKey) return
			if (e.key !== 'Enter' && e.code !== 'NumpadEnter') return
			const t = e.target
			if (t instanceof HTMLElement && t.closest('[role="dialog"]')) return
			e.preventDefault()
			e.stopImmediatePropagation()
			if (!currentQueryText().trim()) return
			void runQuery()
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
				running={runState.status === 'running'}
				{dirty}
				{saving}
				onchange={emitText}
				onrun={() => void runQuery()}
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
				{exportMsg}
				refreshing={runState.status === 'running'}
				onexport={(f) => void exportResults(f)}
				onprev={goResultPrev}
				onnext={goResultNext}
				onrefresh={() => void runQuery()}
			/>
		</div>
	</div>
</div>
