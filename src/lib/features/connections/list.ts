/**
 * The home connection list: filter, order and group by environment. Pure, so
 * the sidebar only renders what this returns.
 */
import type { ConnectionProfile, Environment } from '#core/contracts'
import { byName } from '#core/util'
import type { ConnectionListSort } from '#lib/app'
import { asEnvironment, configStr } from './model'

/**
 * The orders the sort menu offers. The list is always grouped by
 * environment, so an `environment` order saved by an older build reads as
 * `name`.
 */
export const SORT_OPTIONS: readonly { id: ConnectionListSort; label: string }[] = [
	{ id: 'name', label: 'Name' },
	{ id: 'created', label: 'Created' },
	{ id: 'recent', label: 'Recently used' },
	{ id: 'engine', label: 'Connection type' },
]

export function isListSort(id: string): id is ConnectionListSort {
	return SORT_OPTIONS.some((opt) => opt.id === id)
}

export const ENVIRONMENTS: readonly { id: Environment; label: string }[] = [
	{ id: 'production', label: 'Production' },
	{ id: 'staging', label: 'Staging' },
	{ id: 'local', label: 'Local' },
]

export type ConnectionGroup<T extends ConnectionProfile = ConnectionProfile> = {
	env: Environment
	label: string
	items: T[]
}

/** Matches name, engine, host, database or file path, ignoring case. */
export function matchesQuery(profile: ConnectionProfile, query: string): boolean {
	const q = query.trim().toLowerCase()
	if (!q) return true
	const cfg = profile.pluginConfig ?? {}
	return [
		profile.name,
		String(profile.engine),
		configStr(cfg, 'host'),
		configStr(cfg, 'database'),
		configStr(cfg, 'path'),
	].some((field) => field.toLowerCase().includes(q))
}


/** Newest first; a missing or unreadable time sorts last. */
function byTimeDesc(a: string | null | undefined, b: string | null | undefined): number {
	const ta = a ? Date.parse(a) : NaN
	const tb = b ? Date.parse(b) : NaN
	return (Number.isFinite(tb) ? tb : 0) - (Number.isFinite(ta) ? ta : 0)
}

/** Favourites first, then the chosen order, then name. */
export function compareConnections(
	a: ConnectionProfile,
	b: ConnectionProfile,
	sort: ConnectionListSort,
): number {
	if (a.favorite !== b.favorite) return a.favorite ? -1 : 1
	const n =
		sort === 'created'
			? byTimeDesc(a.createdAt, b.createdAt)
			: sort === 'recent'
				? byTimeDesc(a.lastConnectedAt, b.lastConnectedAt)
				: sort === 'engine'
					? String(a.engine).localeCompare(String(b.engine), undefined, {
							sensitivity: 'base',
						})
					: 0
	return n || byName(a, b)
}

/** Filtered, ordered, and grouped Production → Staging → Local; empty groups dropped. */
export function groupConnections<T extends ConnectionProfile>(
	rows: readonly T[],
	query: string,
	sort: ConnectionListSort,
): ConnectionGroup<T>[] {
	const visible = rows
		.filter((row) => matchesQuery(row, query))
		.sort((a, b) => compareConnections(a, b, sort))
	return ENVIRONMENTS.map(({ id, label }) => ({
		env: id,
		label,
		items: visible.filter((row) => asEnvironment(row.environment) === id),
	})).filter((group) => group.items.length > 0)
}

/** Filter words that get a treat instead of "no matches". */
const TREATS = new Set(['woof', 'treat', 'treats', 'bone', 'good boy', 'good girl', 'sit'])

export function isTreatQuery(query: string): boolean {
	return TREATS.has(query.trim().toLowerCase())
}
