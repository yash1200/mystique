import type { WriteGuardPrompt } from '#core/contracts'
import { unlockEngineWrites } from '#core/ipc'
import { guardLoop, type GuardOptions, type WriteGuardDecision } from './guard-loop'

export {
	isWriteCancelled,
	parseWriteGuard,
	WriteCancelled,
	type GuardOptions,
	type WriteGuardDecision,
} from './guard-loop'

let pending: {
	prompt: WriteGuardPrompt
	resolve: (d: WriteGuardDecision) => void
} | null = null
const listeners = new Set<() => void>()

function emit() {
	for (const fn of listeners) fn()
}

export function subscribeWriteGuard(fn: () => void): () => void {
	listeners.add(fn)
	return () => listeners.delete(fn)
}

export function currentWriteGuard(): WriteGuardPrompt | null {
	return pending?.prompt ?? null
}

/** Show the one write dialog. A new prompt cancels the open one. */
export function askWriteGuard(prompt: WriteGuardPrompt): Promise<WriteGuardDecision> {
	return new Promise((resolve) => {
		pending?.resolve({ ok: false })
		pending = { prompt, resolve }
		emit()
	})
}

export function resolveWriteGuard(decision: WriteGuardDecision) {
	pending?.resolve(decision)
	pending = null
	emit()
}

export function runWithWriteGuard<T>(
	sessionId: string,
	run: (confirmObjectName?: string) => Promise<T>,
	options?: GuardOptions,
): Promise<T> {
	return guardLoop(
		run,
		{ ask: askWriteGuard, unlock: () => unlockEngineWrites(sessionId) },
		options,
	)
}
