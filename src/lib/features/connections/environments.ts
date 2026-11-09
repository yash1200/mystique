/**
 * A connection has one environment, set in the connection form; the backend
 * guard judges every write by it. The frontend only needs to know whether
 * it is production (for example, to tell the import dialog).
 */
import type { ConnectionProfile } from '#core/contracts'

type Marked = Pick<ConnectionProfile, 'environment'>

/** The connection is production. Case and spaces do not matter. */
export function hasProduction(profile: Marked): boolean {
	return (profile.environment ?? '').trim().toLowerCase() === 'production'
}
