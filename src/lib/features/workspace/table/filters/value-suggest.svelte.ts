/**
 * Distinct-value suggestions for one table's filter values.
 *
 * Debounced, bounded and cached: the first request for a column loads up to
 * 1000 distinct values; if that is all of them, every later prefix filters
 * locally. Otherwise each prefix asks the server for at most 50 values that
 * start with it. A newer keystroke drops the answer to an older one (the
 * distinct query itself is bounded by LIMIT and a 10 s server timeout).
 */
import { DISTINCT_LIMIT } from './column-complete'
import { matchValues } from './value-complete'

export const VALUE_DEBOUNCE_MS = 200
export const VALUE_PREFIX_LIMIT = 50
export const VALUE_TTL_MS = 30_000
const CACHE_MAX = 64

type Entry = { at: number; values: string[]; complete: boolean }

export type FetchValues = (column: string, prefix: string, limit: number) => Promise<string[]>

export class ValueSuggester {
	items = $state<string[]>([])
	#seq = 0
	#timer: ReturnType<typeof setTimeout> | undefined
	#cache = new Map<string, Entry>()
	readonly #fetch: FetchValues
	readonly #enabled: () => boolean
	readonly #scope: () => string | null
	readonly #now: () => number
	readonly #delay: number

	constructor(opts: {
		fetch: FetchValues
		enabled: () => boolean
		/** Session and table; a new scope starts a new cache key. */
		scope: () => string | null
		now?: () => number
		delayMs?: number
	}) {
		this.#fetch = opts.fetch
		this.#enabled = opts.enabled
		this.#scope = opts.scope
		this.#now = opts.now ?? Date.now
		this.#delay = opts.delayMs ?? VALUE_DEBOUNCE_MS
	}

	/** Suggest values of `column` for what is typed so far. */
	request(column: string, prefix: string) {
		const seq = ++this.#seq
		if (this.#timer) clearTimeout(this.#timer)
		const scope = this.#scope()
		if (!this.#enabled() || !scope || !column) {
			this.items = []
			return
		}
		const local = this.#local(scope, column, prefix)
		if (local) {
			this.items = local
			return
		}
		this.#timer = setTimeout(() => void this.#load(seq, scope, column, prefix), this.#delay)
	}

	clear() {
		this.#seq += 1
		if (this.#timer) clearTimeout(this.#timer)
		this.items = []
	}

	dispose() {
		this.clear()
		this.#cache.clear()
	}

	#key(scope: string, column: string, prefix: string): string {
		return `${scope}\n${column}\n${prefix.toLowerCase()}`
	}

	#fresh(key: string): Entry | null {
		const entry = this.#cache.get(key)
		if (!entry) return null
		if (this.#now() - entry.at > VALUE_TTL_MS) {
			this.#cache.delete(key)
			return null
		}
		return entry
	}

	#store(key: string, values: string[], complete: boolean) {
		this.#cache.delete(key)
		this.#cache.set(key, { at: this.#now(), values, complete })
		while (this.#cache.size > CACHE_MAX) {
			const oldest = this.#cache.keys().next().value
			if (oldest === undefined) break
			this.#cache.delete(oldest)
		}
	}

	#local(scope: string, column: string, prefix: string): string[] | null {
		const all = this.#fresh(this.#key(scope, column, ''))
		if (all && (all.complete || !prefix)) return matchValues(all.values, prefix)
		// A complete answer for a shorter prefix covers this one.
		for (let n = prefix.length; n >= 1; n--) {
			const hit = this.#fresh(this.#key(scope, column, prefix.slice(0, n)))
			if (hit && (hit.complete || n === prefix.length)) return matchValues(hit.values, prefix)
		}
		return null
	}

	async #load(seq: number, scope: string, column: string, prefix: string) {
		const stale = () => seq !== this.#seq
		try {
			const allKey = this.#key(scope, column, '')
			if (!this.#fresh(allKey)) {
				const values = await this.#fetch(column, '', DISTINCT_LIMIT)
				this.#store(allKey, values, values.length < DISTINCT_LIMIT)
				if (stale()) return
				const local = this.#local(scope, column, prefix)
				if (local) {
					this.items = local
					return
				}
			}
			const values = await this.#fetch(column, prefix, VALUE_PREFIX_LIMIT)
			this.#store(this.#key(scope, column, prefix), values, values.length < VALUE_PREFIX_LIMIT)
			if (stale()) return
			this.items = matchValues(values, prefix)
		} catch {
			if (!stale()) this.items = []
		}
	}
}
