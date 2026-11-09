/**
 * Close every engine session, clear workspace state, and hard-load Home.
 * ⌘/Ctrl+Shift+R goes through `resetGuard` first, which asks when edits or
 * transactions would be lost.
 */
import { resolve } from '$app/paths'
import { closeEngineSession, listEngineSessions } from '#core/ipc'
import { workspaceSession } from './session.svelte'
let inFlight = false

export async function resetAppToHome(): Promise<void> {
	if (inFlight) return
	inFlight = true
	try {
		const known = workspaceSession.sessions
			.map((s) => s.engineSessionId)
			.filter((id): id is string => !!id)
		workspaceSession.disconnectAll()

		const ids = new Set(known)
		try {
			const live = await listEngineSessions()
			for (const s of live) {
				if (s.sessionId) ids.add(s.sessionId)
			}
		} catch {
			/* list is best-effort */
		}
		const closes = [...ids].map((id) =>
			closeEngineSession(id).catch(() => {}),
		)
		await Promise.race([
			Promise.all(closes),
			new Promise<void>((r) => setTimeout(r, 2000)),
		])
	} finally {
		window.location.replace(resolve('/'))
	}
}
