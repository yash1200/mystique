/**
 * One execution id per tab. Query, preview, and export share it so Esc,
 * Cancel, and closing the tab stop the same server run.
 */
import { cancelEngineQuery } from './engines'
import { isTauri } from './tauri'

const runs = new Map<string, string>()

export function startTabRun(tabId: string): string {
	const previous = runs.get(tabId)
	const executionId = crypto.randomUUID()
	runs.set(tabId, executionId)
	if (previous && isTauri()) {
		void cancelEngineQuery(previous).catch(() => {
			/* the previous run may already have finished */
		})
	}
	return executionId
}

export function finishTabRun(tabId: string, executionId: string): void {
	if (runs.get(tabId) === executionId) runs.delete(tabId)
}

export async function cancelTabRun(tabId: string): Promise<void> {
	const executionId = runs.get(tabId)
	if (!executionId) return
	runs.delete(tabId)
	if (!isTauri()) return
	try {
		await cancelEngineQuery(executionId)
	} catch {
		/* an unknown id is a no-op on the server */
	}
}
