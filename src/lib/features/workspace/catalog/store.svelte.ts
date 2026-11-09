/**
 * Live catalog state, keyed by connectionId: each open connection has its own
 * databases, load bookkeeping, busy flags and error, so a refresh or a failure
 * on one connection never shows on another.
 */
import type { DatabaseMeta } from '#core/contracts'
import { showToast } from '#features/toast'
import {
	selectedDatabaseName,
	selectedDatabases,
	workspaceSession,
} from '#features/workspace/session'

export type CatalogStatus = {
	/** The database list is loading or refreshing. */
	refreshing: boolean
	/** The current database's objects are reloading. */
	refreshingTables: boolean
	/** A create group / object call is running. */
	createBusy: boolean
	/** Last catalog or table error. */
	error: string | null
}

const IDLE: CatalogStatus = Object.freeze({
	refreshing: false,
	refreshingTables: false,
	createBusy: false,
	error: null,
})

function without<T>(record: Record<string, T>, key: string): Record<string, T> {
	if (!(key in record)) return record
	const { [key]: _drop, ...rest } = record
	return rest
}

class CatalogStore {
	/** connectionId → databases, with the objects loaded so far. */
	catalogs = $state<Record<string, DatabaseMeta[]>>({})
	/** connectionId → the engineSessionId the live catalog was loaded for. */
	liveKey = $state<Record<string, string>>({})
	/** connectionId → a catalog load is in flight. */
	loadInFlight = $state<Record<string, boolean>>({})
	/** connectionId → busy flags and the last error. */
	status = $state<Record<string, CatalogStatus>>({})

	databases(connectionId: string | null | undefined): DatabaseMeta[] {
		return connectionId ? (this.catalogs[connectionId] ?? []) : []
	}

	setDatabases(connectionId: string, list: DatabaseMeta[]) {
		this.catalogs = { ...this.catalogs, [connectionId]: list }
	}

	statusOf(connectionId: string | null | undefined): CatalogStatus {
		return (connectionId && this.status[connectionId]) || IDLE
	}

	patchStatus(connectionId: string, patch: Partial<CatalogStatus>) {
		this.status = {
			...this.status,
			[connectionId]: { ...this.statusOf(connectionId), ...patch },
		}
	}

	/** Set or clear the connection's catalog/table error. Non-null values also toast. */
	setError(connectionId: string, message: string | null) {
		this.patchStatus(connectionId, { error: message })
		if (message) showToast('danger', message)
	}

	/** The next ensure reloads even when the engine session id did not change. */
	invalidate(connectionId: string) {
		this.liveKey = { ...this.liveKey, [connectionId]: '' }
	}

	/** Drop everything held for a closed connection. */
	forget(connectionId: string) {
		this.catalogs = without(this.catalogs, connectionId)
		this.liveKey = without(this.liveKey, connectionId)
		this.loadInFlight = without(this.loadInFlight, connectionId)
		this.status = without(this.status, connectionId)
	}
}

export const catalogStore = new CatalogStore()

/** The database a connection works in: its pick, else its first database. */
export function databaseNameFor(
	connectionId: string | null | undefined,
): string {
	return selectedDatabaseName(
		selectedDatabases.byConnection,
		connectionId,
		catalogStore.databases(connectionId),
	)
}

/** The active connection's database. */
export function currentDatabaseName(): string {
	return databaseNameFor(workspaceSession.activeConnectionId)
}
