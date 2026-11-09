/**
 * Query runs per tab: start, progress, cancel, the write-guard loop, result
 * paging, and the tab's pinned session (manual commit / transaction).
 *
 * The run itself lives in the session's per-tab run cache, so leaving a tab
 * (or unmounting the canvas) never drops an in-flight run or starts a second.
 */
import type { QueryMode } from '#core/contracts'
import {
	cancelTabRun,
	commitPinnedTab,
	errorMessage,
	executeEngineBatch,
	executePinnedBatch,
	finishTabRun,
	pinnedTabStatus,
	pinQueryTab,
	rollbackPinnedTab,
	startTabRun,
	toIpcError,
	unpinQueryTab,
	type PinStatus,
} from '#core/ipc'
import { showToast } from '#features/toast'
import {
	isSessionMissingError,
	isWriteCancelled,
	runWithWriteGuard,
	workspaceSession,
} from '#features/workspace/session'
import { QUERY_FETCH_MAX_ROWS } from '../constants'
import { userFacingQueryError } from '../display'
import type { QueryHistoryItem } from '../history/history-types'
import {
	editorEpoch,
	placeDiagnostic,
	type QueryDiagnostic,
} from '../editor/query-diagnostic'
import { splitDialectFor } from '../editor/statement-span'
import { scanQueryParams, type ParamSlot } from '../params/params'
import { initialActiveIndex } from '../results/batch-view'
import type { QueryRunCache, RunState } from './run-types'

const IDLE: RunState = { status: 'idle' }

/** Everything a run needs from the tab that starts it. */
export type RunTarget = {
	tabId: string
	engine: string
	language: string
	mode: QueryMode
	timeoutMs: number
	/** Live engine session, when there is one. */
	sessionId: string | null
	database: string | null
	/** Mongo collection field. */
	collection: string
	/** The tab runs on its pinned session (manual commit). */
	manualCommit: boolean
	/** Pinned-session status is tracked (the engine has transactions). */
	transactions: boolean
	/** Re-open a dead engine session; returns the new session id. */
	reconnect?: () => Promise<string | null>
	/** A run finished (ok or failed); history has a new row. */
	onhistory?: (item?: QueryHistoryItem | null) => void
	/** The run is going ahead (after the params check). */
	onstart?: () => void
	/** The editor text now, to place an error position. */
	doc: () => string
}

export type RunOutcome =
	| { kind: 'skipped' }
	| { kind: 'done' }
	/** The text has placeholders: ask for values, then run with `bound`. */
	| { kind: 'params'; text: string; origin: number; slots: ParamSlot[] }

function runCache(tabId: string): QueryRunCache | undefined {
	return workspaceSession.getQueryRunCache(tabId)
}

export function runStateOf(tabId: string): RunState {
	return runCache(tabId)?.runState ?? IDLE
}

export function isRunning(tabId: string): boolean {
	return runStateOf(tabId).status === 'running'
}

/**
 * Write the tab's run cache. Fields left out keep their current value, and
 * `diagnostic: undefined` keeps the current diagnostic. A closed tab is left
 * alone.
 */
export function persistRun(
	tabId: string,
	state?: RunState,
	opts: {
		page?: number
		submittedText?: string
		activeIndex?: number
		diagnostic?: QueryDiagnostic | null
		submittedStart?: number
	} = {},
) {
	if (!workspaceSession.hasOpenTab(tabId)) return
	const prev = runCache(tabId)
	workspaceSession.setQueryRunCache(tabId, {
		runState: state ?? prev?.runState ?? IDLE,
		resultPage: opts.page ?? prev?.resultPage ?? 0,
		submittedText: opts.submittedText ?? prev?.submittedText,
		activeIndex: opts.activeIndex ?? prev?.activeIndex ?? 0,
		submittedStart: opts.submittedStart ?? prev?.submittedStart,
		diagnostic: opts.diagnostic === undefined ? prev?.diagnostic : opts.diagnostic,
	})
}

/** The editor changed: an error mark no longer points at the right text. */
export function clearDiagnostic(tabId: string) {
	if (runCache(tabId)?.diagnostic) persistRun(tabId, undefined, { diagnostic: null })
}

export function setResultPage(tabId: string, page: number) {
	persistRun(tabId, undefined, { page })
}

/** Show one statement's result from the last batch. */
export function selectStatement(tabId: string, index: number) {
	const state = runStateOf(tabId)
	if (state.status !== 'ok' || !state.batch) return
	const stmt = state.batch.results[index]
	if (!stmt) return
	persistRun(
		tabId,
		{
			...state,
			message: stmt.error ? '' : (stmt.result?.message ?? '').trim(),
			result: stmt.result ?? null,
		},
		{ page: 0, activeIndex: index },
	)
}

export function cancelRun(tabId: string) {
	if (!isRunning(tabId)) return
	void cancelTabRun(tabId)
}

/** The session to run on, reconnecting once when there is none. */
export async function liveSessionFor(target: RunTarget): Promise<string | null> {
	let sid = target.sessionId
	if (!sid && target.reconnect) sid = await target.reconnect()
	return sid
}

/** A failed run's state: blocked by policy, or an engine error. */
export function failedRun(
	e: unknown,
	mode: QueryMode,
	elapsedMs: number,
): { state: RunState; message: string; fault: ReturnType<typeof toIpcError> } {
	const fault = toIpcError(e)
	const isPolicy = fault.kind === 'policy'
	const message = isPolicy ? fault.message : userFacingQueryError(fault.message)
	return {
		state: { status: isPolicy ? 'blocked' : 'error', message, mode, elapsedMs },
		message,
		fault,
	}
}

function elapsedSince(start: number): number {
	return Math.round(performance.now() - start)
}

/**
 * Run `text` (one statement or a script) on the tab. Asks for parameters
 * first when the text has placeholders and `bound` is not given.
 */
export async function runQuery(
	target: RunTarget,
	text: string,
	origin = 0,
	bound?: unknown[],
): Promise<RunOutcome> {
	const { tabId, mode } = target
	if (isRunning(tabId)) return { kind: 'skipped' }
	const sqlText = text.trim()
	if (!sqlText) return { kind: 'skipped' }
	if (bound === undefined && target.language !== 'mql') {
		const scanned = scanQueryParams(sqlText, splitDialectFor(target.engine))
		if (scanned.error) {
			persistRun(tabId, {
				status: 'error',
				message: scanned.error,
				mode,
				elapsedMs: 0,
			})
			return { kind: 'done' }
		}
		if (scanned.slots.length > 0) {
			return { kind: 'params', text: sqlText, origin, slots: scanned.slots }
		}
		bound = []
	}
	target.onstart?.()
	const start = performance.now()
	const startedEpoch = editorEpoch(tabId)
	persistRun(
		tabId,
		{ status: 'running' },
		{ page: 0, submittedText: sqlText, activeIndex: 0, diagnostic: null, submittedStart: origin },
	)

	const sid = await liveSessionFor(target)
	if (!sid) {
		persistRun(tabId, {
			status: 'error',
			message:
				'No live engine session. Go Home → Connect so a pooled session is opened.',
			mode,
			elapsedMs: elapsedSince(start),
		})
		return { kind: 'done' }
	}

	// Everything below writes to the tab that started the run, even when the
	// canvas shows another tab by the time it ends.
	const manualCommit = target.manualCommit
	const executionId = startTabRun(tabId)
	const diagnosticAt = (
		message: string,
		position: { offset: number; line: number; column: number } | null | undefined,
	): QueryDiagnostic | null => {
		if (!position || editorEpoch(tabId) !== startedEpoch) return null
		return placeDiagnostic(target.doc(), origin, position, message)
	}
	try {
		const request = {
			sessionId: sid,
			text: sqlText,
			mode,
			maxRows: QUERY_FETCH_MAX_ROWS,
			timeoutMs: target.timeoutMs,
			collection:
				target.language === 'mql' ? target.collection.trim() || null : null,
			database: target.database?.trim() ? target.database.trim() : null,
			executionId,
			params: bound ?? [],
		}
		const onProgress = (event: { index: number; total: number }) => {
			if (!workspaceSession.hasOpenTab(tabId)) return
			if (runStateOf(tabId).status !== 'running') return
			persistRun(tabId, {
				status: 'running',
				progress: { index: event.index, total: event.total },
			})
		}
		const runBatch = (sessionId: string, confirmObjectName: string | null) => {
			const body = { ...request, sessionId, confirmObjectName }
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
			if (mode === 'write' || !target.reconnect || !isSessionMissingError(e))
				throw e
			const next = await target.reconnect()
			if (!next) throw e
			batch = await runWithWriteGuard(next, (confirmObjectName) =>
				runBatch(next, confirmObjectName ?? null),
			)
		}
		const elapsedMs = elapsedSince(start)
		if (!workspaceSession.hasOpenTab(tabId)) return { kind: 'done' }
		const index = initialActiveIndex(batch.results, batch.stoppedAt)
		const active = batch.results[index]
		persistRun(
			tabId,
			{
				status: 'ok',
				message: active?.error ? '' : (active?.result?.message ?? '').trim(),
				mode,
				elapsedMs,
				result: active?.result ?? null,
				batch,
			},
			{
				page: 0,
				submittedText: sqlText,
				activeIndex: index,
				diagnostic: active?.error
					? diagnosticAt(active.error, active.errorPosition)
					: null,
				submittedStart: origin,
			},
		)
		target.onhistory?.(null)
	} catch (e) {
		if (!workspaceSession.hasOpenTab(tabId)) return { kind: 'done' }
		if (isWriteCancelled(e)) {
			persistRun(tabId, IDLE)
			return { kind: 'done' }
		}
		const { state, message, fault } = failedRun(e, mode, elapsedSince(start))
		// A failed manual-commit run says whether its transaction survived.
		if (fault.transactionOutcome) applyPinStatus(tabId, fault.transactionOutcome)
		persistRun(tabId, state, {
			page: 0,
			submittedText: sqlText,
			activeIndex: fault.statementIndex ?? 0,
			diagnostic: diagnosticAt(message, fault.position),
			submittedStart: origin,
		})
		target.onhistory?.(null)
	} finally {
		finishTabRun(tabId, executionId)
		if (manualCommit) void refreshPin(tabId, target.transactions)
	}
	return { kind: 'done' }
}

// --- Pinned session (manual commit) -----------------------------------------

/** Tabs already told that the backend dropped their transaction. */
const rollbackNoticed = new Set<string>()

/** Store the tab's pin status; say so once when the backend rolled it back. */
export function applyPinStatus(tabId: string, status: PinStatus) {
	workspaceSession.setTabTxn(tabId, status)
	if (!status.rolledBack) {
		rollbackNoticed.delete(tabId)
		return
	}
	if (rollbackNoticed.has(tabId)) return
	rollbackNoticed.add(tabId)
	showToast(
		'warning',
		'The open transaction was rolled back (idle timeout, lost connection, or the server ended it). Nothing in it was committed.',
	)
}

/** Ask the backend for the tab's pin status. Failures keep the last one. */
async function refreshPin(tabId: string, enabled: boolean) {
	if (!enabled) return
	try {
		applyPinStatus(tabId, await pinnedTabStatus(tabId))
	} catch {
		/* The toolbar keeps the last status. */
	}
}

export async function setManualCommit(
	tabId: string,
	sessionId: string | null,
	manual: boolean,
) {
	if (!sessionId) {
		showToast('danger', 'Connect before changing auto-commit.')
		return
	}
	try {
		const status = manual
			? await pinQueryTab(sessionId, tabId)
			: await unpinQueryTab(tabId)
		workspaceSession.setTabTxn(tabId, {
			pinned: manual ? status.pinned : false,
			inTransaction: status.inTransaction,
		})
		if (!manual && status.rolledBack) showToast('info', 'Open transaction rolled back.')
	} catch (e) {
		showToast('danger', errorMessage(e))
	}
}

export async function commitPin(tabId: string, transactions: boolean) {
	try {
		workspaceSession.setTabTxn(tabId, await commitPinnedTab(tabId))
	} catch (e) {
		showToast('danger', errorMessage(e))
		// A lost transaction leaves the toolbar showing one open.
		void refreshPin(tabId, transactions)
	}
}

export async function rollbackPin(tabId: string, transactions: boolean) {
	try {
		workspaceSession.setTabTxn(tabId, await rollbackPinnedTab(tabId))
	} catch (e) {
		showToast('danger', errorMessage(e))
		// A lost transaction leaves the toolbar showing one open.
		void refreshPin(tabId, transactions)
	}
}
