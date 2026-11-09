/**
 * A per-connection cache: each connection gets its own entries, loading flag
 * and last error, so work on one connection never shows on, or blocks,
 * another. Feature stores (structures, ER models) are instances of this.
 */
export type ConnectionSlot<T> = {
	entries: Record<string, T>
	loading: boolean
	error: string | null
}

function emptySlot<T>(): ConnectionSlot<T> {
	return { entries: {}, loading: false, error: null }
}

const EMPTY = Object.freeze({
	entries: Object.freeze({}),
	loading: false,
	error: null,
}) as ConnectionSlot<never>

export class ConnectionCache<T> {
	/** connectionId → that connection's slot. */
	byConnection = $state<Record<string, ConnectionSlot<T>>>({})

	/** The connection's slot; an empty, idle one when it has none. */
	of(connectionId: string | null | undefined): ConnectionSlot<T> {
		return (connectionId && this.byConnection[connectionId]) || EMPTY
	}

	get(
		connectionId: string | null | undefined,
		key: string | null | undefined,
	): T | undefined {
		if (!key) return undefined
		return this.of(connectionId).entries[key]
	}

	put(connectionId: string, key: string, value: T) {
		const cur = this.of(connectionId)
		this.patch(connectionId, { entries: { ...cur.entries, [key]: value } })
	}

	drop(connectionId: string, key: string) {
		const cur = this.of(connectionId)
		if (!(key in cur.entries)) return
		const { [key]: _drop, ...rest } = cur.entries
		this.patch(connectionId, { entries: rest })
	}

	setLoading(connectionId: string, loading: boolean) {
		this.patch(connectionId, { loading })
	}

	setError(connectionId: string, error: string | null) {
		this.patch(connectionId, { error })
	}

	/** Drop everything held for a closed connection. */
	forget(connectionId: string) {
		if (!(connectionId in this.byConnection)) return
		const { [connectionId]: _drop, ...rest } = this.byConnection
		this.byConnection = rest
	}

	private patch(connectionId: string, next: Partial<ConnectionSlot<T>>) {
		const cur = this.byConnection[connectionId] ?? emptySlot<T>()
		this.byConnection = {
			...this.byConnection,
			[connectionId]: { ...cur, ...next },
		}
	}
}
