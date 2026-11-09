/** Shared catalog-column dump per engine session + database. */

import type { CatalogColumnRow } from '#core/contracts'
import { listEngineCatalogColumns } from '#core/ipc'

function keyOf(sessionId: string, database: string | null | undefined): string {
	return `${sessionId}::${database?.trim() || ''}`
}

class CatalogColumnsCache {
	byKey = $state<Record<string, CatalogColumnRow[]>>({})
	#inflight = new Map<string, Promise<CatalogColumnRow[]>>()
	/**
	 * Bumped by `invalidate` and by a forced reload. A load that finishes
	 * under an older generation is dropped: it describes a catalog the user
	 * has since refreshed or left.
	 */
	#generation = new Map<string, number>()

	key = keyOf

	peek = (
		sessionId: string,
		database: string | null | undefined,
	): CatalogColumnRow[] | undefined => this.byKey[keyOf(sessionId, database)]

	/** The load running for this session and database, if any. */
	pending = (
		sessionId: string,
		database: string | null | undefined,
	): Promise<CatalogColumnRow[]> | null => this.#inflight.get(keyOf(sessionId, database)) ?? null

	load = async (
		sessionId: string,
		database: string | null | undefined,
		force = false,
	): Promise<CatalogColumnRow[]> => {
		const k = keyOf(sessionId, database)
		if (!force && this.byKey[k]) return this.byKey[k]
		const running = this.#inflight.get(k)
		if (running && !force) return running
		const generation = this.#bump(k, force)
		const task = listEngineCatalogColumns({
			sessionId,
			database: database?.trim() || null,
		})
			.then((rows) => {
				const next = rows ?? []
				if (this.#generation.get(k) === generation) {
					this.byKey = { ...this.byKey, [k]: next }
				}
				return next
			})
			.finally(() => {
				if (this.#inflight.get(k) === task) this.#inflight.delete(k)
			})
		this.#inflight.set(k, task)
		return task
	}

	invalidate = (sessionId?: string) => {
		const prefix = sessionId ? `${sessionId}::` : ''
		const next = { ...this.byKey }
		for (const k of Object.keys(next)) {
			if (k.startsWith(prefix)) delete next[k]
		}
		this.byKey = next
		for (const k of [...this.#inflight.keys()]) {
			if (k.startsWith(prefix)) this.#inflight.delete(k)
		}
		for (const k of [...this.#generation.keys()]) {
			if (k.startsWith(prefix)) this.#bump(k, true)
		}
	}

	#bump(k: string, force: boolean): number {
		const current = this.#generation.get(k) ?? 0
		const next = force ? current + 1 : current
		this.#generation.set(k, next)
		return next
	}
}

export const catalogColumnsCache = new CatalogColumnsCache()
