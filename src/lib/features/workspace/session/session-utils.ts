import type { ConnectionProfile } from '#core/contracts'
import { errorMessage } from '#core/ipc'
import { databaseFromUri, defaultCapabilitiesForEngine } from '#engines'
import { configStr, pluginConfigForSession } from '#features/connections'
/** Postgres / MySQL pools bind to one database; switching requires a new session. */
export function engineBindsSessionToDatabase(
	engine: string,
	caps?: { bindsSessionToDatabase?: boolean } | null,
): boolean {
	if (caps && typeof caps.bindsSessionToDatabase === 'boolean') {
		return caps.bindsSessionToDatabase
	}
	return defaultCapabilitiesForEngine(engine).bindsSessionToDatabase
}

function databaseFromUriIfPreferred(
	engine: string,
	cfg: Record<string, unknown>,
): string | null {
	if (!defaultCapabilitiesForEngine(engine).preferDatabaseFromUri) return null
	return databaseFromUri(engine, configStr(cfg, 'uri')) || null
}

export function preferredDbForProfile(profile: {
	engine: string
	pluginConfig?: Record<string, unknown>
}): string | null {
	const engine = String(profile.engine)
	try {
		const cfg = pluginConfigForSession(profile as ConnectionProfile)
		const fromUri = databaseFromUriIfPreferred(engine, cfg)
		if (fromUri) return fromUri
		const explicit = configStr(cfg, 'database').trim()
		if (explicit) return explicit
	} catch {
		/* fall through */
	}
	const cfg = profile.pluginConfig ?? {}
	const fromUri = databaseFromUriIfPreferred(engine, cfg)
	if (fromUri) return fromUri
	const explicit = configStr(cfg, 'database').trim()
	if (explicit) return explicit
	return null
}

/** Pool dropped, idle timeout, or the engine session was closed. */
function isStaleEngineError(err: unknown): boolean {
	const msg = errorMessage(err)
	return (
		/session not found/i.test(msg) ||
		/session closed/i.test(msg) ||
		/connection closed/i.test(msg) ||
		/connection reset/i.test(msg) ||
		/broken pipe/i.test(msg) ||
		/server has gone away/i.test(msg) ||
		/lost connection/i.test(msg) ||
		/pool closed/i.test(msg) ||
		/pool timed? ?out/i.test(msg) ||
		/timeout waiting (for|on) (a )?connection/i.test(msg) ||
		/terminating connection/i.test(msg) ||
		/server closed the connection/i.test(msg) ||
		/could not connect/i.test(msg) ||
		/connection refused/i.test(msg) ||
		/unexpected eof/i.test(msg) ||
		/connection is dead/i.test(msg) ||
		/no connection to the server/i.test(msg)
	)
}

export function isSessionMissingError(err: unknown): boolean {
	return isStaleEngineError(err)
}

export function modelCacheKey(
	connectionId: string,
	database: string,
	tableId?: string | null,
): string {
	return `${connectionId}::${database.trim() || '_'}::${(tableId ?? '').trim() || '_'}`
}
