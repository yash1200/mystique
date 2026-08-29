/** Shared catalog-column dump per engine session + database. */

import type { CatalogColumnRow } from '$core/contracts'
import { listEngineCatalogColumns } from '$core/ipc'

function keyOf(sessionId: string, database: string | null | undefined): string {
	return `${sessionId}::${database?.trim() || ''}`
}

class CatalogColumnsCache {
	byKey = $state<Record<string, CatalogColumnRow[]>>({})
	#inflight = new Map<string, Promise<CatalogColumnRow[]>>()

	key = keyOf

	peek = (
		sessionId: string,
		database: string | null | undefined,
	): CatalogColumnRow[] | undefined => this.byKey[keyOf(sessionId, database)]

	load = async (
		sessionId: string,
		database: string | null | undefined,
		force = false,
	): Promise<CatalogColumnRow[]> => {
		const k = keyOf(sessionId, database)
		if (!force && this.byKey[k]) return this.byKey[k]
		const pending = this.#inflight.get(k)
		if (pending) return pending
		const task = listEngineCatalogColumns({
			sessionId,
			database: database?.trim() || null,
		})
			.then((rows) => {
				const next = rows ?? []
				this.byKey = { ...this.byKey, [k]: next }
				return next
			})
			.finally(() => {
				this.#inflight.delete(k)
			})
		this.#inflight.set(k, task)
		return task
	}

	invalidate = (sessionId?: string) => {
		if (!sessionId) {
			this.byKey = {}
			this.#inflight.clear()
			return
		}
		const prefix = `${sessionId}::`
		const next = { ...this.byKey }
		for (const k of Object.keys(next)) {
			if (k.startsWith(prefix)) delete next[k]
		}
		this.byKey = next
		for (const k of [...this.#inflight.keys()]) {
			if (k.startsWith(prefix)) this.#inflight.delete(k)
		}
	}
}

export const catalogColumnsCache = new CatalogColumnsCache()
