/** EXPLAIN (or EXPLAIN ANALYZE) a statement. The plan goes to the caller. */
import type { ExplainPlan } from '#core/contracts'
import {
	explainEngineQuery,
	finishTabRun,
	isTauri,
	startTabRun,
} from '#core/ipc'
import {
	isWriteCancelled,
	runWithWriteGuard,
	workspaceSession,
} from '#features/workspace/session'
import {
	failedRun,
	isRunning,
	liveSessionFor,
	persistRun,
	type RunTarget,
} from '../run/run.svelte'

/**
 * Explain `text` on the tab. Returns the plan, or null when it failed (the
 * tab's run state then says why) or the tab closed meanwhile.
 */
export async function runExplain(
	target: RunTarget,
	text: string,
	analyze: boolean,
): Promise<ExplainPlan | null> {
	const { tabId, mode } = target
	if (isRunning(tabId)) return null
	const sqlText = text.trim()
	if (!sqlText) return null
	const start = performance.now()
	const elapsed = () => Math.round(performance.now() - start)
	persistRun(
		tabId,
		{ status: 'running' },
		{ page: 0, submittedText: sqlText, activeIndex: 0, diagnostic: null },
	)

	if (!isTauri()) {
		persistRun(tabId, {
			status: 'error',
			message:
				'Explain requires the Mystique desktop app. Connect and try again.',
			mode,
			elapsedMs: elapsed(),
		})
		return null
	}
	const sid = await liveSessionFor(target)
	if (!sid) {
		persistRun(tabId, {
			status: 'error',
			message:
				'No live engine session. Go Home → Connect so a pooled session is opened.',
			mode,
			elapsedMs: elapsed(),
		})
		return null
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
			}),
		)
		if (!workspaceSession.hasOpenTab(tabId)) return null
		persistRun(tabId, { status: 'idle' })
		return plan
	} catch (e) {
		if (!workspaceSession.hasOpenTab(tabId)) return null
		if (isWriteCancelled(e)) {
			persistRun(tabId, { status: 'idle' })
			return null
		}
		persistRun(tabId, failedRun(e, mode, elapsed()).state)
		return null
	} finally {
		finishTabRun(tabId, executionId)
	}
}
