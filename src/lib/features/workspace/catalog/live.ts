/** Live catalog load / refresh / create. Owned by the catalog feature. */
import type { DatabaseMeta, TableMeta } from '#core/contracts'
import { applyEngineTableAdmin, isTauri } from '#core/ipc'
import { resolveCapabilities } from '#engines'
import {
	engineBindsSessionToDatabase,
	isSessionMissingError,
	reopenEngineSession,
	requireActiveConnection,
	runWithWriteGuard,
	sessionDatabase,
	systemDbNames,
	withLiveSession,
	workspaceSession,
	workspaceStudio,
} from '#features/workspace/shell'
import {
	loadLiveCatalog,
	mergeCatalogKeepCached,
	mergeCatalogReplaceDatabase,
	preferDatabase,
} from './api'
import { catalogColumnsCache } from './columns.svelte'
import { clearCatalogFailure, recordCatalogFailure } from './failure'

function reportCatalogError(connectionId: string, err: unknown) {
	const message = err instanceof Error ? err.message : String(err)
	const first = recordCatalogFailure(connectionId, message)
	if (first) {
		workspaceStudio.setCatalogError(message)
		return
	}
	// Same failure again: keep the banner, do not toast in a loop.
	workspaceStudio.catalogError = message
}

export async function ensureLiveCatalog(
	connectionId: string,
	engine: string,
	engineSessionId: string,
	preferredDatabase: string | null,
	force = false,
	includeObjects = true,
) {
	const existing = workspaceStudio.catalogs[connectionId]
	if (
		!force &&
		workspaceStudio.liveCatalogKey[connectionId] === engineSessionId &&
		existing &&
		existing.length > 0
	) {
		return
	}
	if (workspaceStudio.catalogLoadInFlight[connectionId]) return
	if (force) catalogColumnsCache.invalidate(engineSessionId)
	workspaceStudio.catalogLoadInFlight = {
		...workspaceStudio.catalogLoadInFlight,
		[connectionId]: true,
	}
	workspaceStudio.refreshing = true
	workspaceStudio.setCatalogError(null)
	let sessionId = engineSessionId
	try {
		const live = await loadLiveCatalog(sessionId, engine, {
			preferredDatabase,
			includeObjects,
		})
		applyCatalog(connectionId, live, preferredDatabase, engine)
		workspaceStudio.liveCatalogKey = {
			...workspaceStudio.liveCatalogKey,
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
				workspaceStudio.liveCatalogKey = {
					...workspaceStudio.liveCatalogKey,
					[connectionId]: sessionId,
				}
				clearCatalogFailure(connectionId)
				workspaceStudio.setCatalogError(null)
				return
			} catch (retryErr) {
				reportCatalogError(connectionId, retryErr)
			}
		} else {
			reportCatalogError(connectionId, e)
		}
		if (!workspaceStudio.catalogs[connectionId]) {
			workspaceStudio.catalogs = {
				...workspaceStudio.catalogs,
				[connectionId]: [],
			}
		}
	} finally {
		workspaceStudio.catalogLoadInFlight = {
			...workspaceStudio.catalogLoadInFlight,
			[connectionId]: false,
		}
		workspaceStudio.refreshing = false
	}
}

function applyCatalog(
	connectionId: string,
	live: DatabaseMeta[],
	preferredDatabase: string | null,
	engine: string,
) {
	workspaceStudio.catalogs = {
		...workspaceStudio.catalogs,
		[connectionId]: mergeCatalogKeepCached(
			workspaceStudio.catalogs[connectionId],
			live,
		),
	}
	const names = live.map((d) => d.name)
	const pick = preferDatabase(
		names,
		preferredDatabase,
		systemDbNames(engine, connectionId),
	)
	const current = workspaceStudio.selectedDb[connectionId]
	if (current && names.includes(current)) return
	if (pick) {
		workspaceStudio.selectedDb = {
			...workspaceStudio.selectedDb,
			[connectionId]: pick,
		}
	} else if (live[0]) {
		workspaceStudio.selectedDb = {
			...workspaceStudio.selectedDb,
			[connectionId]: live[0].name,
		}
	}
}

export async function ensureCatalogForActiveSession() {
	const active = requireActiveConnection()
	if (!active || !isTauri()) return
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
	if (!next || workspaceStudio.selectedDb[id] === next) return
	const previous = workspaceStudio.selectedDb[id] ?? ''
	workspaceStudio.selectedDb = { ...workspaceStudio.selectedDb, [id]: next }

	const engine = String(session.profile.engine)
	if (!isTauri() || !engineBindsSessionToDatabase(engine, session.capabilities))
		return

	workspaceStudio.refreshing = true
	workspaceStudio.setCatalogError(null)
	try {
		const sid = await reopenEngineSession(session.profile, next)
		await ensureLiveCatalog(id, engine, sid, next, true)
	} catch (e) {
		workspaceStudio.selectedDb = {
			...workspaceStudio.selectedDb,
			[id]: previous,
		}
		workspaceStudio.setCatalogError(
			e instanceof Error ? e.message : String(e),
		)
		workspaceStudio.refreshing = false
	}
}

export async function refreshDatabases() {
	const active = requireActiveConnection()
	if (!active || !isTauri()) return
	const { id, session } = active
	const engine = String(session.profile.engine)
	const preferred = sessionDatabase(id, session.profile)
	workspaceStudio.refreshing = true
	workspaceStudio.setCatalogError(null)
	try {
		const sid = await withLiveSession(id, async (sessionId) => sessionId)
		await ensureLiveCatalog(id, engine, sid, preferred, true, false)
	} catch (e) {
		workspaceStudio.setCatalogError(
			e instanceof Error ? e.message : String(e),
		)
		workspaceStudio.refreshing = false
	}
}

export async function refreshCurrentTables() {
	const active = requireActiveConnection()
	if (!active || !isTauri()) return
	const { id, session } = active
	const engine = String(session.profile.engine)
	const db =
		workspaceStudio.selectedDb[id] || sessionDatabase(id, session.profile) || ''
	workspaceStudio.refreshingTables = true
	workspaceStudio.setCatalogError(null)
	try {
		const sid = await withLiveSession(id, async (sessionId) => sessionId)
		const live = await loadLiveCatalog(sid, engine, {
			preferredDatabase: db,
			includeObjects: true,
		})
		workspaceStudio.catalogs = {
			...workspaceStudio.catalogs,
			[id]: mergeCatalogReplaceDatabase(workspaceStudio.catalogs[id], live, db),
		}
	} catch (e) {
		workspaceStudio.setCatalogError(
			e instanceof Error ? e.message : String(e),
		)
	} finally {
		workspaceStudio.refreshingTables = false
	}
}

export function createDatabase(name: string) {
	const id = workspaceSession.activeConnectionId
	if (!id) return
	const entry: DatabaseMeta = { name, meta: 'created', tables: [] }
	const list = [...(workspaceStudio.catalogs[id] ?? []), entry]
	workspaceStudio.catalogs = { ...workspaceStudio.catalogs, [id]: list }
	workspaceStudio.selectedDb = { ...workspaceStudio.selectedDb, [id]: name }
}

export async function createCatalogGroup(name: string) {
	const active = requireActiveConnection()
	if (!active) return
	const { id, session } = active
	if (!isTauri()) {
		workspaceStudio.setCatalogError(null)
		return
	}
	workspaceStudio.createBusy = true
	workspaceStudio.setCatalogError(null)
	try {
		await withLiveSession(
			id,
			(sid) =>
				runWithWriteGuard(sid, (confirmObjectName) =>
					applyEngineTableAdmin({
						sessionId: sid,
						op: 'createGroup',
						name,
						database: currentDb(id),
						profileReadOnly: !!session.profile.readOnly,
						confirmObjectName: confirmObjectName ?? null,
					}),
				),
			{ replay: false },
		)
		await refreshCurrentTables()
	} catch (e) {
		workspaceStudio.setCatalogError(
			e instanceof Error ? e.message : String(e),
		)
	} finally {
		workspaceStudio.createBusy = false
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
	const dbName = currentDb(id)
	if (!isTauri()) {
		const schema = group || (kind === 'collection' ? '' : 'public')
		const tableId = schema ? `${dbName}.${schema}.${name}` : `${dbName}.${name}`
		const table: TableMeta = {
			id: tableId,
			name,
			schema,
			database: dbName,
			kind: kind as TableMeta['kind'],
			rowCount: 0,
			columns: [],
			rows: [],
		}
		const list = (workspaceStudio.catalogs[id] ?? []).map((d) => {
			if (d.name !== dbName) return d
			if (d.tables.some((t) => t.id === tableId)) return d
			return { ...d, tables: [...d.tables, table] }
		})
		workspaceStudio.catalogs = { ...workspaceStudio.catalogs, [id]: list }
		workspaceStudio.setCatalogError(null)
		return
	}
	workspaceStudio.createBusy = true
	workspaceStudio.setCatalogError(null)
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
						profileReadOnly: !!session.profile.readOnly,
						confirmObjectName: confirmObjectName ?? null,
					}),
				),
			{ replay: false },
		)
		await refreshCurrentTables()
	} catch (e) {
		workspaceStudio.setCatalogError(
			e instanceof Error ? e.message : String(e),
		)
	} finally {
		workspaceStudio.createBusy = false
	}
}

function currentDb(connectionId: string): string {
	return (
		workspaceStudio.selectedDb[connectionId] ??
		workspaceStudio.catalogs[connectionId]?.[0]?.name ??
		''
	)
}
