/**
 * One execution id per run. A tab keeps one slot per kind, so cancelling a
 * preview never stops an export on the same tab. The backend registers the
 * id before any other work and honours a cancel sent before the run starts.
 */
import { cancelEngineQuery } from './engines'

export type RunKind = 'query' | 'preview' | 'export'

const runs = new Map<string, Map<RunKind, string>>()

async function cancelExecution(executionId: string): Promise<void> {
	try {
		await cancelEngineQuery(executionId)
	} catch {
		/* the run may already have finished; an unknown id is a no-op */
	}
}

/** A new run of the same kind on the tab cancels the previous one. */
export function startTabRun(tabId: string, kind: RunKind = 'query'): string {
	const slots = runs.get(tabId) ?? new Map<RunKind, string>()
	const previous = slots.get(kind)
	const executionId = crypto.randomUUID()
	slots.set(kind, executionId)
	runs.set(tabId, slots)
	if (previous) void cancelExecution(previous)
	return executionId
}

export function finishTabRun(tabId: string, executionId: string): void {
	const slots = runs.get(tabId)
	if (!slots) return
	for (const [kind, id] of slots) {
		if (id === executionId) slots.delete(kind)
	}
	if (slots.size === 0) runs.delete(tabId)
}

/** Without a kind, stops every run on the tab (used when it closes). */
export async function cancelTabRun(
	tabId: string,
	kind?: RunKind,
): Promise<void> {
	const slots = runs.get(tabId)
	if (!slots) return
	const ids: string[] = []
	for (const [k, id] of slots) {
		if (kind && k !== kind) continue
		ids.push(id)
		slots.delete(k)
	}
	if (slots.size === 0) runs.delete(tabId)
	await Promise.all(ids.map(cancelExecution))
}
