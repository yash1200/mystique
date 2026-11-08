/**
 * The database each connection works in, keyed by connectionId.
 *
 * It lives in the kernel because reopening an engine session needs it
 * (`withLiveSession` binds the new session to this database). The catalog
 * feature owns the lists of databases; this only remembers the pick.
 */
class SelectedDatabases {
	/** connectionId → database name. */
	byConnection = $state<Record<string, string>>({})

	get(connectionId: string | null | undefined): string | undefined {
		return connectionId ? this.byConnection[connectionId] : undefined
	}

	set(connectionId: string, name: string) {
		this.byConnection = { ...this.byConnection, [connectionId]: name }
	}

	forget(connectionId: string) {
		if (!(connectionId in this.byConnection)) return
		const { [connectionId]: _drop, ...rest } = this.byConnection
		this.byConnection = rest
	}
}

export const selectedDatabases = new SelectedDatabases()
