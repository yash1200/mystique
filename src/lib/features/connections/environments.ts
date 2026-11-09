/**
 * Per-database environment marks (a dev server hosting one production
 * database). The backend guard decides; these helpers only drive the
 * production marker and the explorer menu, using the same rules:
 * a schema mark beats its database's mark, which beats the connection.
 */
import type {
	ConnectionProfile,
	DatabaseEnvironment,
	Environment,
} from '#core/contracts'

type Marked = Pick<ConnectionProfile, 'environment' | 'databaseEnvironments'>

const RANK: Record<Environment, number> = { local: 0, staging: 1, production: 2 }

function parseEnv(raw: string | null | undefined): Environment {
	const env = (raw ?? '').trim().toLowerCase()
	return env === 'production' || env === 'staging' ? env : 'local'
}

function same(a: string | null | undefined, b: string | null | undefined): boolean {
	return (a ?? '').trim().toLowerCase() === (b ?? '').trim().toLowerCase()
}

function marksOf(profile: Marked): DatabaseEnvironment[] {
	return profile.databaseEnvironments ?? []
}

/** The mark on exactly this database (and schema), if any. */
export function databaseMark(
	profile: Marked,
	database: string | null | undefined,
	schema?: string | null,
): DatabaseEnvironment | undefined {
	if (!database?.trim()) return undefined
	return marksOf(profile).find(
		(m) =>
			same(m.database, database) &&
			(schema?.trim() ? same(m.schema, schema) : !m.schema?.trim()),
	)
}

/**
 * The environment writes to `database` (and `schema`) are judged by.
 * No database: the connection environment.
 */
export function databaseEnvironment(
	profile: Marked,
	database: string | null | undefined,
	schema?: string | null,
): Environment {
	const connection = parseEnv(profile.environment)
	if (!database?.trim()) return connection
	const whole = databaseMark(profile, database)
	const base = whole ? parseEnv(whole.environment) : connection
	if (!schema?.trim()) return base
	const own = databaseMark(profile, database, schema)
	return own ? parseEnv(own.environment) : base
}

export function isProductionDatabase(
	profile: Marked,
	database: string | null | undefined,
	schema?: string | null,
): boolean {
	return databaseEnvironment(profile, database, schema) === 'production'
}

/** The connection, or any database on it, is production. */
export function hasProduction(profile: Marked): boolean {
	return (
		parseEnv(profile.environment) === 'production' ||
		marksOf(profile).some((m) => parseEnv(m.environment) === 'production')
	)
}

/** The most dangerous environment on the connection. */
export function worstEnvironment(profile: Marked): Environment {
	let worst = parseEnv(profile.environment)
	for (const mark of marksOf(profile)) {
		const env = parseEnv(mark.environment)
		if (RANK[env] > RANK[worst]) worst = env
	}
	return worst
}

/** Tooltip for the production marker. */
export function productionLabel(
	profile: Pick<ConnectionProfile, 'name'> & Marked,
	database?: string | null,
): string {
	if (database?.trim() && databaseMark(profile, database)) {
		return `Production database “${database.trim()}”. Writes ask first.`
	}
	if (parseEnv(profile.environment) === 'production') {
		return `Production connection “${profile.name}”. Writes ask first.`
	}
	return 'A database on this connection is production. Writes to it ask first.'
}

/**
 * The menu action for a database row: mark it production, or put it back
 * to the connection's environment.
 */
export function markAction(
	profile: Marked,
	database: string,
): { label: string; environment: string | null } {
	const connection = parseEnv(profile.environment)
	const mark = databaseMark(profile, database)
	const production = databaseEnvironment(profile, database) === 'production'
	// Clearing a mark falls back to the connection environment.
	if (mark) {
		return {
			label: production ? 'Remove production mark' : 'Mark as production',
			environment: null,
		}
	}
	if (connection === 'production') {
		return { label: 'Mark as not production', environment: 'local' }
	}
	return { label: 'Mark as production', environment: 'production' }
}
