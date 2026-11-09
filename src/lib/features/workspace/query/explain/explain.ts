/** EXPLAIN (or EXPLAIN ANALYZE) a statement. The plan goes to the caller. */
import type { ExplainPlan } from '#core/contracts'
import {
	explainEngineQuery,
	finishTabRun,
	startTabRun,
} from '#core/ipc'
import {
	isWriteCancelled,
	runWithWriteGuard,
	workspaceSession,
} from '#features/workspace/session'
import { splitDialectFor } from '../editor/statement-span'
import { scanQueryParams, type ParamSlot } from '../params/params'
import {
	failedRun,
	isRunning,
	liveSessionFor,
	persistRun,
	type RunTarget,
} from '../run/run.svelte'

export type ExplainOutcome =
	| { kind: 'plan'; plan: ExplainPlan }
	/** The text has placeholders: ask for values, then explain with `bound`. */
	| { kind: 'params'; text: string; slots: ParamSlot[] }
	/** Failed (the tab's run state says why), skipped, or the tab closed. */
	| { kind: 'none' }

const NONE: ExplainOutcome = { kind: 'none' }

/**
 * Explain `text` on the tab. Placeholders are bound like a normal run: the
 * first call without `bound` asks for their values.
 */
export async function runExplain(
	target: RunTarget,
	text: string,
	analyze: boolean,
	bound?: unknown[],
): Promise<ExplainOutcome> {
	const { tabId, mode } = target
	if (isRunning(tabId)) return NONE
	const sqlText = text.trim()
	if (!sqlText) return NONE
	if (bound === undefined && target.language !== 'mql') {
		const scanned = scanQueryParams(sqlText, splitDialectFor(target.engine))
		if (scanned.error) {
			persistRun(tabId, {
				status: 'error',
				message: scanned.error,
				mode,
				elapsedMs: 0,
			})
			return NONE
		}
		if (scanned.slots.length > 0) {
			return { kind: 'params', text: sqlText, slots: scanned.slots }
		}
	}
	const start = performance.now()
	const elapsed = () => Math.round(performance.now() - start)
	persistRun(
		tabId,
		{ status: 'running' },
		{ page: 0, submittedText: sqlText, activeIndex: 0, diagnostic: null },
	)

	const sid = await liveSessionFor(target)
	if (!sid) {
		persistRun(tabId, {
			status: 'error',
			message:
				'No live engine session. Go Home → Connect so a pooled session is opened.',
			mode,
			elapsedMs: elapsed(),
		})
		return NONE
	}
	const mql = target.language === 'mql'
	const executionId = startTabRun(tabId)
	try {
		const plan = await runWithWriteGuard(sid, (confirmObjectName) =>
			explainEngineQuery({
				sessionId: sid,
				text: sqlText,
				analyze,
				mode,
				timeoutMs: target.timeoutMs,
				confirmObjectName: confirmObjectName ?? null,
				collection: mql ? target.collection.trim() || null : null,
				database: mql && target.database?.trim() ? target.database.trim() : null,
				executionId,
				params: bound ?? [],
			}),
		)
		if (!workspaceSession.hasOpenTab(tabId)) return NONE
		persistRun(tabId, { status: 'idle' })
		return { kind: 'plan', plan }
	} catch (e) {
		if (!workspaceSession.hasOpenTab(tabId)) return NONE
		if (isWriteCancelled(e)) {
			persistRun(tabId, { status: 'idle' })
			return NONE
		}
		persistRun(tabId, failedRun(e, mode, elapsed()).state)
		return NONE
	} finally {
		finishTabRun(tabId, executionId)
	}
}
