/**
 * Load the catalogs of open sessions that have none yet, so a search across
 * sessions finds their tables. Lives with Quick Open (a lazy chunk), not in
 * the catalog feature, which loads at startup.
 *
 * Quiet: no toasts, no reconnect or password prompt; a session without a
 * live engine session is skipped. At most `concurrency` loads run at once
 * and none starts after `signal` aborts. Loads already running finish and
 * fill the store.
 */
import {
	applyLiveCatalog,
	catalogStore,
	loadLiveCatalog,
} from '#features/workspace/catalog'
import { sessionDatabase, workspaceSession } from '#features/workspace/session'

export async function warmSessionCatalogs(
	connectionIds: readonly string[],
	signal: AbortSignal,
	concurrency = 2,
): Promise<void> {
	const queue = connectionIds.filter((id) => needsCatalog(id) != null)
	const worker = async () => {
		for (;;) {
			if (signal.aborted) return
			const id = queue.shift()
			if (!id) return
			await warmOne(id)
		}
	}
	await Promise.all(Array.from({ length: Math.max(1, concurrency) }, worker))
}

/** The live engine session id when this connection has no catalog yet. */
function needsCatalog(connectionId: string): string | null {
	const session = workspaceSession.sessions.find((s) => s.profile.id === connectionId)
	const sid = session?.engineSessionId
	if (!sid) return null
	if (catalogStore.catalogs[connectionId] || catalogStore.loadInFlight[connectionId]) return null
	return sid
}

async function warmOne(connectionId: string): Promise<void> {
	const sid = needsCatalog(connectionId)
	const session = workspaceSession.sessions.find((s) => s.profile.id === connectionId)
	if (!sid || !session) return
	const engine = String(session.profile.engine)
	const preferred = sessionDatabase(connectionId, session.profile)
	catalogStore.loadInFlight = { ...catalogStore.loadInFlight, [connectionId]: true }
	try {
		const live = await loadLiveCatalog(sid, engine, { preferredDatabase: preferred })
		// The session may have closed, reconnected or loaded meanwhile.
		const still = workspaceSession.sessions.find((s) => s.profile.id === connectionId)
		if (still?.engineSessionId !== sid || catalogStore.catalogs[connectionId]) return
		applyLiveCatalog(connectionId, live, preferred, engine)
		catalogStore.liveKey = { ...catalogStore.liveKey, [connectionId]: sid }
	} catch {
		// Opening that connection loads it again and reports the error.
	} finally {
		catalogStore.loadInFlight = { ...catalogStore.loadInFlight, [connectionId]: false }
	}
}
