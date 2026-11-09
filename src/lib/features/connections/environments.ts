/**
 * The production marker. A connection has one environment, set in the
 * connection form; the backend guard judges every write by it. These
 * helpers only drive the shield shown wherever the connection appears.
 */
import type { ConnectionProfile } from '#core/contracts'

type Marked = Pick<ConnectionProfile, 'environment'>

/** The connection is production. Case and spaces do not matter. */
export function hasProduction(profile: Marked): boolean {
	return (profile.environment ?? '').trim().toLowerCase() === 'production'
}

/** Tooltip for the production marker. */
export function productionLabel(profile: Pick<ConnectionProfile, 'name'>): string {
	return `Production connection “${profile.name}”. Writes ask first.`
}
