/**
 * Engine-session helpers used by catalog / grid / query / ER.
 * Not UI — WorkspaceShell only mounts chrome around these.
 */
import type { ConnectionProfile, SessionLiveStatus } from '#core/contracts'
import { getEngineSession, isTauri } from '#core/ipc'
import { resolveCapabilities } from '#engines'
import {
	ensureSessionWithPassword,
	openSessionWithPassword,
	pluginConfigForSession,
} from '#features/connections'
import { isSessionMissingError, preferredDbForProfile } from './session-utils'
import { workspaceSession } from './session.svelte'
import { selectedDatabases } from './selected-db.svelte'

const HEALTH_INTERVAL_MS = 15_000

export function liveStatusMark(live: SessionLiveStatus | undefined): {
	icon: 'plug' | 'plug-off' | 'refresh' | 'alert-circle'
	class: string
	label: string
	word: string
} {
	switch (live) {
		case 'reconnecting':
			return {
				icon: 'refresh',
				class: 'text-info',
				label: 'Reconnecting',
				word: 'Reconnecting',
			}
		case 'error':
			return {
				icon: 'alert-circle',
				class: 'text-danger',
				label: 'Reconnect failed',
				word: 'Error',
			}
		case 'disconnected':
			return {
				icon: 'plug-off',
				class: 'text-fg-muted',
				label: 'Disconnected — reconnects on next request',
				word: 'Disconnected',
			}
		default:
			return {
				icon: 'plug',
				class: 'text-success',
				label: 'Connected',
				word: 'Connected',
			}
	}
}

export function sessionDatabase(
	connectionId: string,
	profile: ConnectionProfile,
): string | null {
	const selected = selectedDatabases.get(connectionId)?.trim()
	if (selected) return selected
	return preferredDbForProfile(profile)
}

export function systemDbNames(engine: string, connectionId?: string): string[] {
	const session = connectionId
		? workspaceSession.sessions.find((s) => s.profile.id === connectionId)
		: workspaceSession.activeSession
	return resolveCapabilities(engine, session?.capabilities).systemDatabases
}

export async function reopenEngineSession(
	profile: ConnectionProfile,
	database?: string | null,
): Promise<string> {
	const pluginConfig = pluginConfigForSession(profile)
	const db = database?.trim()
	if (db) pluginConfig.database = db
	const request = {
		engine: String(profile.engine),
		pluginConfig,
		pool: { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 },
		sessionId: profile.id,
		profileId: profile.id,
	}
	// A profile that does not save its password is asked once per session.
	const session = db
		? await openSessionWithPassword(request, profile.name)
		: await ensureSessionWithPassword(request, profile.name)
	workspaceSession.connect(profile, session.sessionId, session.capabilities)
	return session.sessionId
}

export async function withLiveSession<T>(
	connectionId: string,
	fn: (sessionId: string) => Promise<T>,
	options?: { replay?: boolean },
): Promise<T> {
	const session = workspaceSession.sessions.find(
		(s) => s.profile.id === connectionId,
	)
	if (!session) {
		throw new Error('No connection in this workspace.')
	}
	const db = sessionDatabase(connectionId, session.profile)
	let sid = session.engineSessionId
	if (!sid) {
		sid = await reopenTracked(connectionId, session.profile, db)
	}
	try {
		const result = await fn(sid)
		workspaceSession.setLiveStatus(connectionId, 'connected')
		return result
	} catch (e) {
		// A write that already reached the server must not run again.
		if (options?.replay === false || !isSessionMissingError(e)) throw e
		sid = await reopenTracked(connectionId, session.profile, db)
		const result = await fn(sid)
		workspaceSession.setLiveStatus(connectionId, 'connected')
		return result
	}
}

async function reopenTracked(
	connectionId: string,
	profile: ConnectionProfile,
	database?: string | null,
): Promise<string> {
	workspaceSession.setLiveStatus(connectionId, 'reconnecting')
	try {
		return await reopenEngineSession(profile, database)
	} catch (e) {
		workspaceSession.setLiveStatus(connectionId, 'error')
		throw e
	}
}

/** Cheap in-memory check: does the backend still have this pool? No DB ping. */
export async function probeSessionHealth(connectionId: string): Promise<void> {
	if (!isTauri()) return
	const session = workspaceSession.sessions.find(
		(s) => s.profile.id === connectionId,
	)
	if (!session || session.liveStatus === 'reconnecting') return
	const sid = session.engineSessionId?.trim()
	if (!sid) {
		workspaceSession.setLiveStatus(connectionId, 'disconnected')
		return
	}
	try {
		const info = await getEngineSession(sid)
		const latest = workspaceSession.sessions.find(
			(s) => s.profile.id === connectionId,
		)
		if (!latest || latest.liveStatus === 'reconnecting') return
		if (info) {
			workspaceSession.setLiveStatus(connectionId, 'connected')
			return
		}
		if (latest.liveStatus !== 'error') {
			workspaceSession.setLiveStatus(connectionId, 'disconnected')
		}
	} catch {
		const latest = workspaceSession.sessions.find(
			(s) => s.profile.id === connectionId,
		)
		if (!latest || latest.liveStatus === 'reconnecting') return
		if (latest.liveStatus !== 'error') {
			workspaceSession.setLiveStatus(connectionId, 'disconnected')
		}
	}
}

/** Poll + focus/visibility probes. Pair with an $effect on the active session id. */
export function startSessionHealthWatch(): () => void {
	const tick = () => {
		const id = workspaceSession.activeConnectionId
		if (id) void probeSessionHealth(id)
	}
	const timer = setInterval(tick, HEALTH_INTERVAL_MS)
	const onVis = () => {
		if (document.visibilityState === 'visible') tick()
	}
	document.addEventListener('visibilitychange', onVis)
	window.addEventListener('focus', tick)
	return () => {
		clearInterval(timer)
		document.removeEventListener('visibilitychange', onVis)
		window.removeEventListener('focus', tick)
	}
}

export async function reconnectActiveSession(): Promise<string | null> {
	const id = workspaceSession.activeConnectionId
	if (!id || !workspaceSession.activeSession) return null
	return withLiveSession(id, async (sid) => sid)
}

export function requireActiveConnection(): {
	id: string
	session: NonNullable<typeof workspaceSession.activeSession>
} | null {
	const id = workspaceSession.activeConnectionId
	const session = workspaceSession.activeSession
	if (!id || !session) return null
	return { id, session }
}

export { isTauri }
