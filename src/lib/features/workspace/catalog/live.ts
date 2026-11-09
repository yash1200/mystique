/** Live catalog load / refresh / create. Owned by the catalog feature. */
import type { DatabaseMeta } from '#core/contracts'
import { errorMessage, applyEngineTableAdmin } from '#core/ipc'
import { resolveCapabilities } from '#engines'
import { isConnectCancelled } from '#features/connections'
import {
	engineBindsSessionToDatabase,
	isSessionMissingError,
	reopenEngineSession,
	requireActiveConnection,
	isWriteCancelled,
	runWithWriteGuard,
	sessionDatabase,
	systemDbNames,
	selectedDatabases,
	withLiveSession,
	workspaceSession,
} from '#features/workspace/session'
import {
	loadLiveCatalog,
	mergeCatalogKeepCached,
	mergeCatalogReplaceDatabase,
	preferDatabase,
} from './api'
import { catalogColumnsCache } from './columns.svelte'
import { clearCatalogFailure, recordCatalogFailure } from './failure'
import { catalogStore, databaseNameFor } from './store.svelte'

function reportCatalogError(connectionId: string, err: unknown) {
	// The user closed the password prompt.
	if (isConnectCancelled(err)) return
	const message = errorMessage(err)
	const first = recordCatalogFailure(connectionId, message)
	if (first) {
		catalogStore.setError(connectionId, message)
		return
	}
	// Same failure again: keep the banner, do not toast in a loop.
	catalogStore.patchStatus(connectionId, { error: message })
}

export async function ensureLiveCatalog(
	connectionId: string,
	engine: string,
	engineSessionId: string,
	preferredDatabase: string | null,
	force = false,
	includeObjects = true,
) {
	const existing = catalogStore.catalogs[connectionId]
	if (
		!force &&
		catalogStore.liveKey[connectionId] === engineSessionId &&
		existing &&
		existing.length > 0
	) {
		return
	}
	if (catalogStore.loadInFlight[connectionId]) return
	if (force) catalogColumnsCache.invalidate(engineSessionId)
	catalogStore.loadInFlight = {
		...catalogStore.loadInFlight,
		[connectionId]: true,
	}
	catalogStore.patchStatus(connectionId, { refreshing: true })
	catalogStore.setError(connectionId, null)
	let sessionId = engineSessionId
	try {
		const live = await loadLiveCatalog(sessionId, engine, {
			preferredDatabase,
			includeObjects,
		})
		applyCatalog(connectionId, live, preferredDatabase, engine)
		catalogStore.liveKey = {
			...catalogStore.liveKey,
			[connectionId]: sessionId,
		}
		clearCatalogFailure(connectionId)
	} catch (e) {
		const session = workspaceSession.sessions.find(
			(s) => s.profile.id === connectionId,
		)
		if (session && isSessionMissingError(e)) {
			try {
				sessionId = await reopenEngineSession(
					session.profile,
					sessionDatabase(connectionId, session.profile),
				)
				const live = await loadLiveCatalog(sessionId, engine, {
					preferredDatabase,
					includeObjects,
				})
				applyCatalog(connectionId, live, preferredDatabase, engine)
				catalogStore.liveKey = {
					...catalogStore.liveKey,
					[connectionId]: sessionId,
				}
				clearCatalogFailure(connectionId)
				catalogStore.setError(connectionId, null)
				return
			} catch (retryErr) {
				reportCatalogError(connectionId, retryErr)
			}
		} else {
			reportCatalogError(connectionId, e)
		}
		if (!catalogStore.catalogs[connectionId]) {
			catalogStore.setDatabases(connectionId, [])
		}
	} finally {
		catalogStore.loadInFlight = {
			...catalogStore.loadInFlight,
			[connectionId]: false,
		}
		catalogStore.patchStatus(connectionId, { refreshing: false })
	}
}

function applyCatalog(
	connectionId: string,
	live: DatabaseMeta[],
	preferredDatabase: string | null,
	engine: string,
) {
	catalogStore.setDatabases(
		connectionId,
		mergeCatalogKeepCached(catalogStore.catalogs[connectionId], live),
	)
	const names = live.map((d) => d.name)
	const pick = preferDatabase(
		names,
		preferredDatabase,
		systemDbNames(engine, connectionId),
	)
	const current = selectedDatabases.get(connectionId)
	if (current && names.includes(current)) return
	if (pick) {
		selectedDatabases.set(connectionId, pick)
	} else if (live[0]) {
		selectedDatabases.set(connectionId, live[0].name)
	}
}

export async function ensureCatalogForActiveSession() {
	const active = requireActiveConnection()
	if (!active) return
	const { id, session } = active
	const engine = String(session.profile.engine)
	const preferred = sessionDatabase(id, session.profile)
	let sid = session.engineSessionId
	if (!sid) {
		try {
			sid = await reopenEngineSession(session.profile, preferred)
		} catch (e) {
			reportCatalogError(id, e)
			return
		}
	}
	await ensureLiveCatalog(id, engine, sid, preferred)
}

export async function selectDatabase(name: string) {
	const active = requireActiveConnection()
	if (!active) return
	const { id, session } = active
	const next = name.trim()
	if (!next || selectedDatabases.get(id) === next) return
	const previous = selectedDatabases.get(id) ?? ''
	selectedDatabases.set(id, next)

	const engine = String(session.profile.engine)
	if (!engineBindsSessionToDatabase(engine, session.capabilities))
		return

	catalogStore.patchStatus(id, { refreshing: true })
	catalogStore.setError(id, null)
	try {
		const sid = await reopenEngineSession(session.profile, next)
		await ensureLiveCatalog(id, engine, sid, next, true)
	} catch (e) {
		selectedDatabases.set(id, previous)
		catalogStore.setError(id, errorMessage(e))
		catalogStore.patchStatus(id, { refreshing: false })
	}
}

export async function refreshDatabases() {
	const active = requireActiveConnection()
	if (!active) return
	const { id, session } = active
	const engine = String(session.profile.engine)
	const preferred = sessionDatabase(id, session.profile)
	catalogStore.patchStatus(id, { refreshing: true })
	catalogStore.setError(id, null)
	try {
		const sid = await withLiveSession(id, async (sessionId) => sessionId)
		await ensureLiveCatalog(id, engine, sid, preferred, true, false)
	} catch (e) {
		catalogStore.setError(id, errorMessage(e))
		catalogStore.patchStatus(id, { refreshing: false })
	}
}

export async function refreshCurrentTables() {
	const active = requireActiveConnection()
	if (!active) return
	const { id, session } = active
	const engine = String(session.profile.engine)
	const db =
		selectedDatabases.get(id) || sessionDatabase(id, session.profile) || ''
	catalogStore.patchStatus(id, { refreshingTables: true })
	catalogStore.setError(id, null)
	try {
		const sid = await withLiveSession(id, async (sessionId) => sessionId)
		const live = await loadLiveCatalog(sid, engine, {
			preferredDatabase: db,
			includeObjects: true,
		})
		catalogStore.setDatabases(
			id,
			mergeCatalogReplaceDatabase(catalogStore.catalogs[id], live, db),
		)
	} catch (e) {
		catalogStore.setError(id, errorMessage(e))
	} finally {
		catalogStore.patchStatus(id, { refreshingTables: false })
	}
}

export function createDatabase(name: string) {
	const id = workspaceSession.activeConnectionId
	if (!id) return
	const entry: DatabaseMeta = { name, meta: 'created', tables: [] }
	catalogStore.setDatabases(id, [...catalogStore.databases(id), entry])
	selectedDatabases.set(id, name)
}

export async function createCatalogGroup(name: string) {
	const active = requireActiveConnection()
	if (!active) return
	const { id } = active
	catalogStore.patchStatus(id, { createBusy: true })
	catalogStore.setError(id, null)
	try {
		await withLiveSession(
			id,
			(sid) =>
				runWithWriteGuard(sid, (confirmObjectName) =>
					applyEngineTableAdmin({
						sessionId: sid,
						op: 'createGroup',
						name,
						database: databaseNameFor(id),
						confirmObjectName: confirmObjectName ?? null,
					}),
				),
			{ replay: false },
		)
		await refreshCurrentTables()
	} catch (e) {
		if (!isWriteCancelled(e)) catalogStore.setError(id, errorMessage(e))
	} finally {
		catalogStore.patchStatus(id, { createBusy: false })
	}
}

export async function createCatalogObject(name: string, group?: string) {
	const active = requireActiveConnection()
	if (!active) return
	const { id, session } = active
	const caps = resolveCapabilities(
		String(session.profile.engine),
		session.capabilities,
	)
	const kind =
		caps.explorer.objectLabelSingular === 'collection' ? 'collection' : 'table'
	const dbName = databaseNameFor(id)
	catalogStore.patchStatus(id, { createBusy: true })
	catalogStore.setError(id, null)
	try {
		await withLiveSession(
			id,
			(sid) =>
				runWithWriteGuard(sid, (confirmObjectName) =>
					applyEngineTableAdmin({
						sessionId: sid,
						op: 'createObject',
						name,
						schema: group || null,
						database: dbName,
						kind,
						confirmObjectName: confirmObjectName ?? null,
					}),
				),
			{ replay: false },
		)
		await refreshCurrentTables()
	} catch (e) {
		if (!isWriteCancelled(e)) catalogStore.setError(id, errorMessage(e))
	} finally {
		catalogStore.patchStatus(id, { createBusy: false })
	}
}
