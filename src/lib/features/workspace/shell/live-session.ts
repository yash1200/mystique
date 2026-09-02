/**
 * Engine-session helpers used by catalog / grid / query / ER.
 * Not UI — WorkspaceShell only mounts chrome around these.
 */
import type { ConnectionProfile } from '$core/contracts'
import { ensureEngineSession, isTauri, openEngineSession } from '$core/ipc'
import { resolveCapabilities } from '$engines'
import { pluginConfigForSession } from '$features/connections'
import { isSessionMissingError, preferredDbForProfile } from './session-utils'
import { workspaceSession } from './session.svelte'
import { workspaceStudio } from './studio.svelte'

export function sessionDatabase(
	connectionId: string,
	profile: ConnectionProfile,
): string | null {
	const selected = workspaceStudio.selectedDb[connectionId]?.trim()
	if (selected) return selected
	return preferredDbForProfile(profile)
}

export function systemDbNames(engine: string, connectionId?: string): string[] {
	const session = connectionId
		? workspaceSession.sessions.find((s) => s.profile.id === connectionId)
		: workspaceSession.activeSession
	return resolveCapabilities(engine, session?.capabilities).systemDatabases
}

export function currentDatabaseName(): string {
	const id = workspaceSession.activeConnectionId
	if (!id) return ''
	const catalogs = workspaceStudio.catalogs[id] ?? []
	return workspaceStudio.selectedDb[id] ?? catalogs[0]?.name ?? ''
}

export async function reopenEngineSession(
	profile: ConnectionProfile,
	database?: string | null,
): Promise<string> {
	const pluginConfig = pluginConfigForSession(profile)
	const db = database?.trim()
	if (db) pluginConfig.database = db
	const password = profile.password?.trim() || null
	const request = {
		engine: String(profile.engine),
		pluginConfig,
		password,
		pool: { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 },
		sessionId: profile.id,
	}
	const session = db
		? await openEngineSession(request)
		: await ensureEngineSession(request)
	workspaceSession.connect(profile, session.sessionId, session.capabilities)
	return session.sessionId
}

export async function withLiveSession<T>(
	connectionId: string,
	fn: (sessionId: string) => Promise<T>,
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
		sid = await reopenEngineSession(session.profile, db)
	}
	try {
		return await fn(sid)
	} catch (e) {
		if (!isSessionMissingError(e)) throw e
		sid = await reopenEngineSession(session.profile, db)
		return await fn(sid)
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
