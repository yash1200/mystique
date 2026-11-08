/**
 * Save a table tab from outside its grid (close prompt, import prompt).
 * The mounted grid registers its save; only the active tab's grid is mounted.
 */

export type SaveOutcome = { ok: true } | { ok: false; error: string | null }
export type TabSaver = () => Promise<SaveOutcome>

const savers = new Map<string, TabSaver>()
const waiting = new Map<string, Array<(saver: TabSaver) => void>>()

export const TAB_NOT_OPEN = 'Open the table tab to save its edits.'

export function registerTabSaver(tabId: string, saver: TabSaver): () => void {
	savers.set(tabId, saver)
	const queued = waiting.get(tabId) ?? []
	waiting.delete(tabId)
	for (const resolve of queued) resolve(saver)
	return () => {
		if (savers.get(tabId) === saver) savers.delete(tabId)
	}
}

/** The tab's saver, waiting up to `timeoutMs` for its grid to mount. */
export function tabSaver(tabId: string, timeoutMs = 10_000): Promise<TabSaver | null> {
	const ready = savers.get(tabId)
	if (ready) return Promise.resolve(ready)
	return new Promise((resolve) => {
		const done = (saver: TabSaver | null) => {
			clearTimeout(timer)
			resolve(saver)
		}
		const timer = setTimeout(() => {
			const list = waiting.get(tabId)?.filter((item) => item !== done) ?? []
			if (list.length > 0) waiting.set(tabId, list)
			else waiting.delete(tabId)
			done(null)
		}, timeoutMs)
		waiting.set(tabId, [...(waiting.get(tabId) ?? []), done])
	})
}

/** Runs the grid's own save (write guard included). */
export async function saveTableTab(tabId: string, timeoutMs?: number): Promise<SaveOutcome> {
	const saver = await tabSaver(tabId, timeoutMs)
	if (!saver) return { ok: false, error: TAB_NOT_OPEN }
	try {
		return await saver()
	} catch (err) {
		return { ok: false, error: err instanceof Error ? err.message : String(err) }
	}
}
