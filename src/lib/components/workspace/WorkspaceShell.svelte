<script lang="ts">
	import { goto } from '$app/navigation'
	import { resolve } from '$app/paths'
	import {
		isMockCatalog,
		loadLiveCatalog,
		loadTablePreview,
		mergeCatalogKeepCached,
		preferDatabase,
	} from '$lib/api/catalog'
	import {
		closeEngineSession,
		openEngineSession,
		testEngineConnection,
		type TablePreviewFilter,
	} from '$lib/api/engines'
	import {
		listQueryHistory,
		listRecentQueryHistory,
		type QueryHistoryItem,
	} from '$lib/api/history'
	import { loadLiveErDiagram } from '$lib/api/liveEr'
	import { touchProfileConnected } from '$lib/api/profiles'
	import {
		deleteSavedQuery,
		listSavedQueries,
		saveQuery,
		toListSavedQuery,
	} from '$lib/api/queries'
	import { isTauri } from '$lib/api/tauri'
	import {
		historyItemToEntry,
		type QueryHistoryEntry,
		type SavedQuery,
	} from '$lib/data/mock-queries'
	import { mockCatalogForEngine } from '$lib/data/mock-workspace'
	import { workspaceSession } from '$lib/stores/workspace-session.svelte'
	import {
		colorHex,
		draftFromProfile,
		pluginConfigForSession,
		type ConnectionProfile,
	} from '$lib/types/connection'
	import type { ErDiagramState } from '$lib/types/er'
	import type { DatabaseMeta, TableMeta } from '$lib/types/workspace'
	import { onMount } from 'svelte'
	import OpenConnectionModal from './OpenConnectionModal.svelte'
	import SaveQueryModal from './SaveQueryModal.svelte'
	import SessionsRail, { type SideNavMode } from './SessionsRail.svelte'
	import {
		engineBindsSessionToDatabase,
		isSessionMissingError,
		modelCacheKey,
		preferredDbForProfile,
	} from './shell/sessionUtils'
	import WorkspaceMainPane from './shell/WorkspaceMainPane.svelte'
	import { mountWorkspaceShortcuts } from './shell/workspaceShortcuts'
	import WorkspaceSidePane from './shell/WorkspaceSidePane.svelte'
	import UnsavedCloseModal from './UnsavedCloseModal.svelte'

	const sessions = $derived(workspaceSession.sessions)
	const activeId = $derived(workspaceSession.activeConnectionId)
	const activeSession = $derived(workspaceSession.activeSession)
	const openTabs = $derived(workspaceSession.openTabs)
	const activeTabId = $derived(workspaceSession.activeTabId)
	const selectedRow = $derived(workspaceSession.selectedRow)

	let catalogs = $state<Record<string, DatabaseMeta[]>>({})
	let selectedDb = $state<Record<string, string>>({})
	let refreshing = $state(false)
	let tableLoading = $state(false)
	let catalogError = $state<string | null>(null)
	let rowSyncTick = $state(0)
	let baselineEpoch = $state(0)
	/** connectionId → last engineSessionId we loaded a live catalog for */
	let liveCatalogKey = $state<Record<string, string>>({})
	/** Prevent overlapping catalog loads for the same connection. */
	let catalogLoadInFlight = $state<Record<string, boolean>>({})

	/** Second-pane mode: schema explorer vs saved queries vs history. */
	let sideMode = $state<SideNavMode>('explorer')
	let selectedQueryId = $state<string | null>(null)
	let selectedHistoryId = $state<string | null>(null)
	let queryHistory = $state<QueryHistoryEntry[]>([])
	let historyLoading = $state(false)
	let historyError = $state<string | null>(null)
	/** Monotonic token so overlapping history fetches don't clobber newer data. */
	let historyFetchGen = 0
	/** Saved queries for the active connection (from app SQLite). */
	let savedQueries = $state<SavedQuery[]>([])
	let querySaving = $state(false)

	/** Live ER / model diagrams keyed by connectionId::database */
	let liveModels = $state<
		Record<
			string,
			{ diagram: ErDiagramState; title: string; summary: string; rev: number }
		>
	>({})
	let modelLoading = $state(false)
	let modelError = $state<string | null>(null)

	/** Open / re-open a pooled engine session for a profile (fixes stale old connections). */
	async function reopenEngineSession(
		profile: ConnectionProfile,
		database?: string | null,
	): Promise<string> {
		const pluginConfig = pluginConfigForSession(profile)
		const db = database?.trim()
		if (db) pluginConfig.database = db
		const password = profile.password?.trim() || null
		const session = await openEngineSession({
			engine: String(profile.engine),
			pluginConfig,
			password,
			pool: { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 },
			sessionId: profile.id,
		})
		workspaceSession.connect(profile, session.sessionId)
		return session.sessionId
	}

	function sessionDatabase(
		connectionId: string,
		profile: ConnectionProfile,
	): string | null {
		const selected = selectedDb[connectionId]?.trim()
		if (selected) return selected
		return preferredDbForProfile(profile)
	}

	/** Run against the live pool; reopen once if the session died. */
	async function withLiveSession<T>(
		connectionId: string,
		fn: (sessionId: string) => Promise<T>,
	): Promise<T> {
		const session = sessions.find((s) => s.profile.id === connectionId)
		if (!session) {
			throw new Error('No connection in this workspace.')
		}
		const db = sessionDatabase(connectionId, session.profile)
		let sid = session.engineSessionId
		if (!sid) {
			sid = await reopenEngineSession(session.profile, db)
		}
		try {
			return await fn(sid)
		} catch (e) {
			if (!isSessionMissingError(e)) throw e
			sid = await reopenEngineSession(session.profile, db)
			return await fn(sid)
		}
	}

	async function reconnectActiveSession(): Promise<string | null> {
		if (!activeId || !activeSession) return null
		return withLiveSession(activeId, async (sid) => sid)
	}

	async function ensureLiveCatalog(
		connectionId: string,
		engine: string,
		engineSessionId: string,
		preferredDatabase: string | null,
		force = false,
	) {
		const existing = catalogs[connectionId]
		const mockStuck = isMockCatalog(engine, existing)
		if (
			!force &&
			!mockStuck &&
			liveCatalogKey[connectionId] === engineSessionId &&
			existing &&
			existing.length > 0
		) {
			return
		}
		if (catalogLoadInFlight[connectionId]) return
		catalogLoadInFlight = { ...catalogLoadInFlight, [connectionId]: true }
		refreshing = true
		catalogError = null
		let sessionId = engineSessionId
		try {
			const live = await loadLiveCatalog(sessionId, engine, {
				preferredDatabase,
			})
			catalogs = {
				...catalogs,
				[connectionId]: mergeCatalogKeepCached(catalogs[connectionId], live),
			}
			liveCatalogKey = { ...liveCatalogKey, [connectionId]: sessionId }
			const names = live.map((d) => d.name)
			const pick = preferDatabase(names, preferredDatabase)
			const current = selectedDb[connectionId]
			if (current && names.includes(current)) {
				// Keep the database the user switched to.
			} else if (pick) {
				selectedDb = { ...selectedDb, [connectionId]: pick }
			} else if (live[0]) {
				selectedDb = { ...selectedDb, [connectionId]: live[0].name }
			}
		} catch (e) {
			// Stale pool session (common for older workspace tabs): reopen and retry once.
			const session = sessions.find((s) => s.profile.id === connectionId)
			if (session && isSessionMissingError(e)) {
				try {
					sessionId = await reopenEngineSession(
						session.profile,
						sessionDatabase(connectionId, session.profile),
					)
					const live = await loadLiveCatalog(sessionId, engine, {
						preferredDatabase,
					})
					catalogs = {
						...catalogs,
						[connectionId]: mergeCatalogKeepCached(
							catalogs[connectionId],
							live,
						),
					}
					liveCatalogKey = { ...liveCatalogKey, [connectionId]: sessionId }
					const names = live.map((d) => d.name)
					const pick = preferDatabase(names, preferredDatabase)
					const current = selectedDb[connectionId]
					if (current && names.includes(current)) {
						// Keep the database the user switched to.
					} else if (pick) {
						selectedDb = { ...selectedDb, [connectionId]: pick }
					}
					catalogError = null
					return
				} catch (retryErr) {
					catalogError =
						retryErr instanceof Error ? retryErr.message : String(retryErr)
				}
			} else {
				catalogError = e instanceof Error ? e.message : String(e)
			}
			// Do not leave mock data sitting as if it were live for Tauri sessions.
			if (
				!catalogs[connectionId] ||
				isMockCatalog(engine, catalogs[connectionId])
			) {
				catalogs = {
					...catalogs,
					[connectionId]: [],
				}
			}
		} finally {
			catalogLoadInFlight = { ...catalogLoadInFlight, [connectionId]: false }
			refreshing = false
		}
	}

	$effect(() => {
		const session = activeSession
		if (!session) return
		const id = session.profile.id
		const engine = String(session.profile.engine)
		const engineSessionId = session.engineSessionId ?? null
		const preferred = sessionDatabase(id, session.profile)

		// Catalog only — do not auto-open a Query tab (empty workspace is intentional).
		if (!isTauri()) {
			// Browser preview only.
			if (!catalogs[id]) {
				catalogs = {
					...catalogs,
					[id]: structuredClone(mockCatalogForEngine(engine)),
				}
			}
			const list = catalogs[id] ?? []
			if (list.length > 0 && !selectedDb[id]) {
				selectedDb = {
					...selectedDb,
					[id]:
						preferDatabase(
							list.map((d) => d.name),
							preferred,
						) || list[0].name,
				}
			}
			return
		}

		// Live Tauri: ensure engine session + live catalog (including old connections).
		void (async () => {
			let sid = engineSessionId
			if (!sid) {
				try {
					sid = await reopenEngineSession(
						session.profile,
						sessionDatabase(id, session.profile),
					)
				} catch (e) {
					catalogError = e instanceof Error ? e.message : String(e)
					return
				}
			}
			await ensureLiveCatalog(id, engine, sid, preferred)
		})()
	})

	const databases = $derived(activeId ? (catalogs[activeId] ?? []) : [])

	const currentDatabaseName = $derived(
		activeId ? (selectedDb[activeId] ?? databases[0]?.name ?? '') : '',
	)

	/** Tabs belonging to the active connection (query + tables). */
	const connectionTabs = $derived(
		activeId ? openTabs.filter((t) => t.connectionId === activeId) : [],
	)

	const activeTab = $derived(openTabs.find((t) => t.id === activeTabId) ?? null)

	const gridTable = $derived.by(() => {
		if (!activeTab || activeTab.kind !== 'table' || !activeTab.tableId) {
			return null
		}
		const cat = catalogs[activeTab.connectionId]
		if (!cat) {
			const session = sessions.find(
				(s) => s.profile.id === activeTab.connectionId,
			)
			if (!session) return null
			const list = mockCatalogForEngine(String(session.profile.engine))
			for (const d of list) {
				const t = d.tables.find((x) => x.id === activeTab.tableId)
				if (t) return t
			}
			return null
		}
		for (const d of cat) {
			const t = d.tables.find((x) => x.id === activeTab.tableId)
			if (t) return t
		}
		return null
	})

	const activeTableIdForExplorer = $derived(
		activeTab &&
			activeTab.kind === 'table' &&
			activeTab.connectionId === activeId
			? (activeTab.tableId ?? null)
			: null,
	)

	const rowSelectionIndex = $derived.by(() => {
		if (!selectedRow || !activeTab || activeTab.kind !== 'table') return null
		if (
			selectedRow.connectionId === activeTab.connectionId &&
			selectedRow.tableId === activeTab.tableId
		) {
			return selectedRow.rowIndex
		}
		return null
	})

	const inspectorData = $derived.by(() => {
		if (sideMode !== 'explorer') return null
		if (rowSelectionIndex === null || !gridTable) return null
		const row = gridTable.rows[rowSelectionIndex]
		if (!row) return null
		const title = gridTable.schema
			? `${gridTable.database}.${gridTable.schema}.${gridTable.name}`
			: `${gridTable.database}.${gridTable.name}`
		return {
			tableTitle: title,
			columns: gridTable.columns,
			row,
			rowIndex: rowSelectionIndex,
		}
	})

	async function refreshSavedQueries(connectionId: string | null) {
		if (!connectionId || !isTauri()) {
			savedQueries = []
			return
		}
		try {
			const rows = await listSavedQueries(connectionId)
			savedQueries = rows.map(toListSavedQuery)
		} catch (e) {
			console.warn('[mystique] list saved queries:', e)
			savedQueries = []
		}
	}

	/** Tab id waiting on the unsaved-close modal. */
	let pendingCloseTabId = $state<string | null>(null)
	let closePromptBusy = $state(false)

	const pendingCloseTab = $derived(
		pendingCloseTabId
			? (workspaceSession.openTabs.find((t) => t.id === pendingCloseTabId) ??
					null)
			: null,
	)

	function requestCloseTab(tabId: string) {
		const tab = workspaceSession.openTabs.find((t) => t.id === tabId)
		if (!tab) return
		if (tab.dirty) {
			// Focus the tab so Save/Discard apply to the visible content.
			workspaceSession.setActiveTab(tabId)
			pendingCloseTabId = tabId
			return
		}
		workspaceSession.closeTab(tabId)
	}

	function cancelCloseTab() {
		if (closePromptBusy) return
		pendingCloseTabId = null
	}

	/** In-app save-name modal (window.prompt is broken in Tauri webviews). */
	let saveNameModalOpen = $state(false)
	let saveNameTabId = $state<string | null>(null)
	let saveNameInitial = $state('Untitled query')
	let saveNameError = $state<string | null>(null)
	/** Resolves the pending saveQueryTab() promise after modal confirm/cancel. */
	let saveNameResolver: ((ok: boolean) => void) | null = null
	/** When true, close the tab after a successful save from the name modal. */
	let saveThenClose = $state(false)

	function needsNamePrompt(title: string): boolean {
		const name = title?.trim() || ''
		if (!name) return true
		if (/^untitled query/i.test(name)) return true
		if (name === 'Query') return true
		return false
	}

	/**
	 * Persist query body to SQLite for the tab. Returns true on success.
	 * Uses an in-app modal for naming (not window.prompt).
	 */
	async function persistQueryTab(
		tabId: string,
		name: string,
	): Promise<boolean> {
		const tab = workspaceSession.openTabs.find((t) => t.id === tabId)
		const session = workspaceSession.sessions.find(
			(s) => s.profile.id === tab?.connectionId,
		)
		if (!tab || tab.kind !== 'query' || !session) {
			throw new Error('No query tab / connection to save.')
		}
		if (!isTauri()) {
			throw new Error('Saving requires the desktop app (Tauri).')
		}
		const body = workspaceSession.queryDrafts[tab.id] ?? ''
		if (!body.trim()) {
			throw new Error('Query is empty — nothing to save.')
		}
		const engine = String(session.profile.engine)
		const language = /mongo/i.test(engine) ? 'mql' : 'sql'
		const saved = await saveQuery({
			id: tab.savedQueryId ?? null,
			connectionId: tab.connectionId,
			name: name.trim(),
			body,
			language,
		})
		workspaceSession.markQuerySaved(tab.id, {
			id: saved.id,
			name: saved.name,
			body: saved.body,
		})
		await refreshSavedQueries(tab.connectionId)
		selectedQueryId = saved.id
		return true
	}

	function closeSaveNameModal(ok: boolean) {
		saveNameModalOpen = false
		saveNameError = null
		const resolve = saveNameResolver
		saveNameResolver = null
		saveNameTabId = null
		resolve?.(ok)
	}

	async function onSaveNameConfirm(name: string) {
		const tabId = saveNameTabId
		if (!tabId) {
			closeSaveNameModal(false)
			return
		}
		querySaving = true
		saveNameError = null
		try {
			await persistQueryTab(tabId, name)
			const shouldClose = saveThenClose
			saveThenClose = false
			closeSaveNameModal(true)
			if (shouldClose) {
				workspaceSession.closeTab(tabId)
				pendingCloseTabId = null
			}
		} catch (e) {
			saveNameError = e instanceof Error ? e.message : String(e)
			console.warn('[mystique] save query:', e)
		} finally {
			querySaving = false
		}
	}

	/**
	 * Save a query tab (by id). Returns true if saved (or nothing to save).
	 * Returns false if the user cancelled the name modal or save failed.
	 */
	async function saveQueryTab(
		tabId: string,
		opts?: { thenClose?: boolean },
	): Promise<boolean> {
		const tab = workspaceSession.openTabs.find((t) => t.id === tabId)
		const session = workspaceSession.sessions.find(
			(s) => s.profile.id === tab?.connectionId,
		)
		if (!tab || tab.kind !== 'query' || !session) return false
		if (!isTauri()) {
			alert('Saving requires the desktop app (run with Tauri).')
			return false
		}
		const body = workspaceSession.queryDrafts[tab.id] ?? ''
		if (!body.trim()) {
			// Empty body: treat as clean so close can proceed.
			workspaceSession.discardQueryDraft(tab.id)
			return true
		}

		if (needsNamePrompt(tab.title)) {
			saveNameTabId = tabId
			saveNameInitial =
				tab.title &&
				!/^untitled query/i.test(tab.title) &&
				tab.title !== 'Query'
					? tab.title
					: 'Untitled query'
			saveNameError = null
			saveThenClose = !!opts?.thenClose
			saveNameModalOpen = true
			return await new Promise<boolean>((resolve) => {
				saveNameResolver = resolve
			})
		}

		querySaving = true
		try {
			await persistQueryTab(tabId, tab.title.trim())
			return true
		} catch (e) {
			console.warn('[mystique] save query:', e)
			alert(e instanceof Error ? e.message : String(e))
			return false
		} finally {
			querySaving = false
		}
	}

	async function saveActiveQuery() {
		const tab = workspaceSession.activeTab
		if (!tab || tab.kind !== 'query') return
		await saveQueryTab(tab.id)
	}

	async function confirmSaveAndClose() {
		const tab = pendingCloseTab
		if (!tab) return
		closePromptBusy = true
		try {
			if (tab.kind === 'query') {
				// If naming modal is needed, it will close the tab after save.
				const ok = await saveQueryTab(tab.id, { thenClose: true })
				if (!ok) return
				// Named query path already saved; close here if still open.
				if (workspaceSession.openTabs.some((t) => t.id === tab.id)) {
					pendingCloseTabId = null
					workspaceSession.closeTab(tab.id)
				} else {
					pendingCloseTabId = null
				}
			} else {
				// Table / model: keep in-memory edits, clear dirty, then close.
				workspaceSession.setTabDirty(tab.id, false)
				const id = tab.id
				pendingCloseTabId = null
				workspaceSession.closeTab(id)
			}
		} finally {
			closePromptBusy = false
		}
	}

	async function confirmDiscardAndClose() {
		const tab = pendingCloseTab
		if (!tab) return
		closePromptBusy = true
		try {
			if (tab.kind === 'query') {
				workspaceSession.discardQueryDraft(tab.id)
			} else if (tab.kind === 'table' && tab.tableId) {
				// Reload table data from the engine to drop local edits.
				const session = workspaceSession.sessions.find(
					(s) => s.profile.id === tab.connectionId,
				)
				const cat = catalogs[tab.connectionId]
				let table: TableMeta | null = null
				if (cat) {
					for (const d of cat) {
						const t = d.tables.find((x) => x.id === tab.tableId)
						if (t) {
							table = t
							break
						}
					}
				}
				if (table && session) {
					try {
						if (isTauri() && session.engineSessionId) {
							const preview = await loadTablePreview(
								session.engineSessionId,
								String(session.profile.engine),
								table,
								100,
							)
							const list = catalogs[tab.connectionId] ?? []
							catalogs = {
								...catalogs,
								[tab.connectionId]: list.map((db) => ({
									...db,
									tables: db.tables.map((t) =>
										t.id === preview.id ? preview : t,
									),
								})),
							}
						}
					} catch (e) {
						console.warn('[mystique] discard table reload:', e)
					}
				}
				workspaceSession.setTabDirty(tab.id, false)
				baselineEpoch += 1
			} else {
				workspaceSession.setTabDirty(tab.id, false)
			}
			const id = tab.id
			pendingCloseTabId = null
			workspaceSession.closeTab(id)
		} finally {
			closePromptBusy = false
		}
	}
	function historyNameMap(): Map<string, string> {
		return new Map(sessions.map((s) => [s.profile.id, s.profile.name] as const))
	}

	function entryFromItem(it: QueryHistoryItem): QueryHistoryEntry {
		const nameById = historyNameMap()
		return historyItemToEntry(
			it,
			(it.connectionId && nameById.get(it.connectionId)) || undefined,
		)
	}

	/** Immediate UI update after a successful append (all engines). */
	function prependHistoryItem(item: QueryHistoryItem | null | undefined) {
		if (!item?.id) {
			void refreshHistory({ preferSelectLatest: true })
			return
		}
		const entry = entryFromItem(item)
		// Only show for active connection (or global if none).
		if (activeId && entry.connectionId && entry.connectionId !== activeId) {
			return
		}
		queryHistory = [
			entry,
			...queryHistory.filter((h) => h.id !== entry.id),
		].slice(0, 150)
		historyError = null
		// Highlight the new run in the list; user opens it explicitly if desired.
		selectedHistoryId = entry.id
	}

	async function refreshHistory(opts?: { preferSelectLatest?: boolean }) {
		if (!isTauri()) {
			queryHistory = []
			return
		}
		const gen = ++historyFetchGen
		historyLoading = true
		try {
			let items: QueryHistoryItem[] = []
			if (activeId) {
				items = await listQueryHistory(activeId, 150)
				// If connection-scoped list is empty, still try recent and filter —
				// covers older rows / edge cases after engine switches.
				if (items.length === 0) {
					const recent = await listRecentQueryHistory(150)
					items = recent.filter((it) => it.connectionId === activeId)
				}
			} else {
				items = await listRecentQueryHistory(150)
			}
			if (gen !== historyFetchGen) return
			queryHistory = items.map((it) => entryFromItem(it))
			historyError = null
			// Keep list highlight only — do not auto-open runs in the editor.
			if (
				selectedHistoryId &&
				!queryHistory.some((h) => h.id === selectedHistoryId)
			) {
				selectedHistoryId = null
			}
			if (opts?.preferSelectLatest && queryHistory[0]) {
				selectedHistoryId = queryHistory[0].id
			}
		} catch (e) {
			if (gen !== historyFetchGen) return
			// Keep existing rows; surface the error instead of wiping the list.
			historyError = e instanceof Error ? e.message : String(e)
			console.warn('[mystique] refreshHistory failed:', historyError)
		} finally {
			if (gen === historyFetchGen) historyLoading = false
		}
	}

	$effect(() => {
		// Reload history when connection changes or history pane is focused.
		const _id = activeId
		const mode = sideMode
		if (mode === 'history' || _id) {
			void refreshHistory({ preferSelectLatest: mode === 'history' })
		}
	})

	/** Open SQL in a new query tab (history / external). */
	function openSqlInEditor(sql: string) {
		if (!activeId) return
		workspaceSession.openNewQuery(activeId)
		const tabId = workspaceSession.activeTabId
		if (tabId) workspaceSession.setQueryDraft(tabId, sql)
	}

	function onSelectHistory(id: string) {
		selectedHistoryId = id
		const h = queryHistory.find((x) => x.id === id)
		if (!h || !activeId) return
		// Open the run directly in the query editor (no preview pane).
		openSqlInEditor(h.sql)
	}

	async function openTable(
		table: TableMeta,
		offset = 0,
		filters: TablePreviewFilter[] = [],
	) {
		if (!activeId || !activeSession) return
		sideMode = 'explorer'
		workspaceSession.openTable(activeId, table.id, table.name)

		if (!isTauri()) return

		// Always fetch live preview so reseed / external writes show up.
		tableLoading = true
		catalogError = null
		try {
			const engine = String(activeSession.profile.engine)
			const preview = await withLiveSession(activeId, (sid) =>
				loadTablePreview(sid, engine, table, 100, offset, filters),
			)
			const list = catalogs[activeId] ?? []
			const next = list.map((db) => ({
				...db,
				tables: db.tables.map((t) => (t.id === table.id ? preview : t)),
			}))
			catalogs = { ...catalogs, [activeId]: next }
			rowSyncTick += 1
			// Do not log table previews / catalog fetches to query history —
			// history is only for statements the user runs from the Query editor.
		} catch (e) {
			catalogError = e instanceof Error ? e.message : String(e)
		} finally {
			tableLoading = false
		}
	}

	async function selectDatabase(name: string) {
		if (!activeId || !activeSession) return
		const next = name.trim()
		if (!next || selectedDb[activeId] === next) return
		const previous = selectedDb[activeId] ?? ''
		selectedDb = { ...selectedDb, [activeId]: next }

		const engine = String(activeSession.profile.engine)
		if (!isTauri() || !engineBindsSessionToDatabase(engine)) return

		refreshing = true
		catalogError = null
		try {
			const sid = await reopenEngineSession(activeSession.profile, next)
			await ensureLiveCatalog(activeId, engine, sid, next, true)
		} catch (e) {
			selectedDb = { ...selectedDb, [activeId]: previous }
			catalogError = e instanceof Error ? e.message : String(e)
			refreshing = false
		}
	}

	/** Open live ER / model diagram for the currently selected database. */
	async function openModelDiagram(force = false) {
		if (!activeSession || !activeId) return
		sideMode = 'explorer'
		const engine = String(activeSession.profile.engine)
		const database = currentDatabaseName || ''
		const title =
			engine.toLowerCase() === 'mongodb' || engine.toLowerCase() === 'mongo'
				? `Model · ${database || 'db'}`
				: `ER · ${database || 'schema'}`

		const key = modelCacheKey(activeId, database)
		const alreadyOpen =
			activeTab?.kind === 'model' &&
			(activeTab.modelDatabase ?? '') === (database.trim() || '')
		workspaceSession.openModel(activeId, database, title)

		// Reuse cache when switching back to the tab; force rebuild when user
		// clicks “ER diagram” while already viewing this model.
		if (!force && !alreadyOpen && liveModels[key]?.diagram?.nodes?.length) {
			modelError = null
			return
		}

		if (!isTauri()) {
			modelError =
				'Live ER diagram requires Tauri. Run `npm run tauri dev` and Connect.'
			return
		}

		modelLoading = true
		modelError = null
		try {
			let sid = activeSession.engineSessionId
			if (!sid) {
				sid = await reopenEngineSession(
					activeSession.profile,
					sessionDatabase(activeId, activeSession.profile),
				)
			}
			const loaded = await loadLiveErDiagram(sid, engine, database || null)
			liveModels = {
				...liveModels,
				[key]: {
					diagram: loaded.diagram,
					title: loaded.title,
					summary: loaded.summary,
					rev: Date.now(),
				},
			}
			workspaceSession.openModel(activeId, database, loaded.title)
		} catch (e) {
			// Session missing — reopen once
			if (isSessionMissingError(e)) {
				try {
					const sid = await reopenEngineSession(
						activeSession.profile,
						sessionDatabase(activeId, activeSession.profile),
					)
					const loaded = await loadLiveErDiagram(sid, engine, database || null)
					liveModels = {
						...liveModels,
						[key]: {
							diagram: loaded.diagram,
							title: loaded.title,
							summary: loaded.summary,
							rev: Date.now(),
						},
					}
					workspaceSession.openModel(activeId, database, loaded.title)
					modelError = null
					return
				} catch (retryErr) {
					modelError =
						retryErr instanceof Error ? retryErr.message : String(retryErr)
					return
				}
			}
			modelError = e instanceof Error ? e.message : String(e)
		} finally {
			modelLoading = false
		}
	}

	const activeModel = $derived.by(() => {
		if (!activeTab || activeTab.kind !== 'model' || !activeId) return null
		const db = activeTab.modelDatabase ?? currentDatabaseName ?? ''
		return liveModels[modelCacheKey(activeId, db)] ?? null
	})

	async function refreshDatabases() {
		if (!activeSession || !activeId) return
		const engine = String(activeSession.profile.engine)
		const preferred = sessionDatabase(activeId, activeSession.profile)

		if (!isTauri()) {
			refreshing = true
			await new Promise((r) => setTimeout(r, 200))
			const base = structuredClone(mockCatalogForEngine(engine))
			catalogs = { ...catalogs, [activeId]: base }
			if (!base.some((d) => d.name === selectedDb[activeId])) {
				selectedDb = { ...selectedDb, [activeId]: base[0]?.name ?? '' }
			}
			refreshing = false
			return
		}

		// Always re-open the pool session so *old* connections pick up URI/DB fixes.
		liveCatalogKey = { ...liveCatalogKey, [activeId]: '' }
		refreshing = true
		catalogError = null
		try {
			const sid = await reopenEngineSession(
				activeSession.profile,
				sessionDatabase(activeId, activeSession.profile),
			)
			await ensureLiveCatalog(activeId, engine, sid, preferred, true)
		} catch (e) {
			catalogError = e instanceof Error ? e.message : String(e)
			refreshing = false
		}
	}

	function createDatabase(name: string) {
		if (!activeId) return
		const entry: DatabaseMeta = {
			name,
			meta: 'created',
			tables: [],
		}
		const list = [...(catalogs[activeId] ?? []), entry]
		catalogs = { ...catalogs, [activeId]: list }
		selectedDb = { ...selectedDb, [activeId]: name }
	}

	let pickerOpen = $state(false)
	let pickerBusy = $state(false)
	let pickerError = $state<string | null>(null)

	const openConnectionIds = $derived(sessions.map((s) => s.profile.id))

	async function connectAdditional(profile: ConnectionProfile) {
		if (sessions.some((s) => s.profile.id === profile.id)) {
			onSelectConnection(profile.id)
			pickerOpen = false
			return
		}
		pickerBusy = true
		pickerError = null
		try {
			if (!isTauri()) {
				workspaceSession.connect({ ...profile, status: 'connected' }, null)
				sideMode = 'explorer'
				pickerOpen = false
				return
			}
			const draft = draftFromProfile(profile)
			const pluginConfig = pluginConfigForSession(profile)
			const password =
				draft.password.trim().length > 0
					? draft.password
					: profile.password?.trim() || null
			const result = await testEngineConnection({
				engine: String(profile.engine),
				pluginConfig,
				password,
				pool: { maxSize: 2, minIdle: 0, acquireTimeoutMs: 8000 },
			})
			if (!result.ok) {
				pickerError = `Could not connect: ${result.message}`
				return
			}
			const updated = await touchProfileConnected(profile.id)
			const session = await openEngineSession({
				engine: String(profile.engine),
				pluginConfig,
				password,
				pool: { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 },
				sessionId: profile.id,
			})
			workspaceSession.connect(
				{ ...updated, status: 'connected' },
				session.sessionId,
			)
			sideMode = 'explorer'
			pickerOpen = false
		} catch (e) {
			pickerError = e instanceof Error ? e.message : String(e)
		} finally {
			pickerBusy = false
		}
	}

	async function refreshConnection(id: string) {
		onSelectConnection(id)
		await refreshDatabases()
	}

	function openNewQueryOn(id: string) {
		onSelectConnection(id)
		workspaceSession.openNewQuery(id)
	}

	function handleDisconnect(id: string) {
		const session = sessions.find((s) => s.profile.id === id)
		const sid = session?.engineSessionId
		workspaceSession.disconnect(id)
		const { [id]: _c, ...restCat } = catalogs
		const { [id]: _d, ...restDb } = selectedDb
		const { [id]: _k, ...restKey } = liveCatalogKey
		const { [id]: _f, ...restFlight } = catalogLoadInFlight
		catalogs = restCat
		selectedDb = restDb
		liveCatalogKey = restKey
		catalogLoadInFlight = restFlight
		const nextModels: typeof liveModels = {}
		for (const [k, v] of Object.entries(liveModels)) {
			if (!k.startsWith(`${id}::`)) nextModels[k] = v
		}
		liveModels = nextModels
		if (sid && isTauri()) {
			void closeEngineSession(sid).catch(() => {})
		}
		if (workspaceSession.sessions.length === 0) goto(resolve('/'))
	}

	function setSideMode(mode: SideNavMode) {
		sideMode = mode
		if (mode === 'saved-queries') {
			selectedHistoryId = null
			void refreshSavedQueries(activeId)
			// Keep editor visible; do not auto-open the first query.
		} else if (mode === 'history') {
			selectedQueryId = null
			void refreshHistory({ preferSelectLatest: true })
		}
	}

	function onSelectSavedQuery(id: string) {
		selectedQueryId = id
		const q = savedQueries.find((s) => s.id === id)
		if (!q || !activeId) return
		// Open directly in the query editor (no preview pane).
		workspaceSession.openSavedQuery(activeId, {
			id: q.id,
			name: q.name,
			body: q.sql,
		})
	}

	/** Rename library query (context menu). */
	let renameModalOpen = $state(false)
	let renameQueryId = $state<string | null>(null)
	let renameInitial = $state('')
	let renameError = $state<string | null>(null)
	let renameBusy = $state(false)

	/** Delete confirmation (context menu). */
	let deleteModalOpen = $state(false)
	let deleteQueryId = $state<string | null>(null)
	let deleteQueryName = $state('')
	let deleteBusy = $state(false)

	async function onSavedQueryAction(
		action: 'duplicate' | 'rename' | 'delete',
		id: string,
	) {
		const q = savedQueries.find((s) => s.id === id)
		if (!q || !activeId) return

		if (action === 'duplicate') {
			try {
				const base = q.name.trim() || 'Query'
				const name = base.toLowerCase().endsWith(' copy')
					? `${base} 2`
					: `${base} copy`
				const saved = await saveQuery({
					connectionId: q.connectionId || activeId,
					name,
					body: q.sql,
					language: /mongo/i.test(String(activeSession?.profile.engine))
						? 'mql'
						: 'sql',
				})
				await refreshSavedQueries(activeId)
				selectedQueryId = saved.id
				workspaceSession.openSavedQuery(activeId, {
					id: saved.id,
					name: saved.name,
					body: saved.body,
				})
			} catch (e) {
				alert(e instanceof Error ? e.message : String(e))
			}
			return
		}

		if (action === 'rename') {
			renameQueryId = id
			renameInitial = q.name
			renameError = null
			renameModalOpen = true
			return
		}

		if (action === 'delete') {
			deleteQueryId = id
			deleteQueryName = q.name
			deleteModalOpen = true
		}
	}

	async function confirmRenameSavedQuery(name: string) {
		const id = renameQueryId
		const q = id ? savedQueries.find((s) => s.id === id) : null
		if (!id || !q || !activeId) {
			renameModalOpen = false
			return
		}
		renameBusy = true
		renameError = null
		try {
			const saved = await saveQuery({
				id,
				connectionId: q.connectionId || activeId,
				name,
				body: q.sql,
				language: /mongo/i.test(String(activeSession?.profile.engine))
					? 'mql'
					: 'sql',
			})
			workspaceSession.renameSavedQueryTabs(id, saved.name)
			await refreshSavedQueries(activeId)
			selectedQueryId = saved.id
			renameModalOpen = false
			renameQueryId = null
		} catch (e) {
			renameError = e instanceof Error ? e.message : String(e)
		} finally {
			renameBusy = false
		}
	}

	async function confirmDeleteSavedQuery() {
		const id = deleteQueryId
		if (!id || !activeId) {
			deleteModalOpen = false
			return
		}
		deleteBusy = true
		try {
			await deleteSavedQuery(id)
			workspaceSession.closeTabsForSavedQuery(id)
			if (selectedQueryId === id) selectedQueryId = null
			await refreshSavedQueries(activeId)
			deleteModalOpen = false
			deleteQueryId = null
		} catch (e) {
			alert(e instanceof Error ? e.message : String(e))
		} finally {
			deleteBusy = false
		}
	}

	// Refresh library when active connection changes.
	$effect(() => {
		const id = activeId
		void refreshSavedQueries(id)
	})

	/** Table tab ids we already tried to hydrate after restore (avoid reload loops). */
	let hydratedTableTabs = $state<Record<string, boolean>>({})

	// After reconnect: hydrate table data for restored table tabs once.
	$effect(() => {
		const tab = activeTab
		const table = gridTable
		const session = activeSession
		if (!tab || tab.kind !== 'table' || !table || !session || !isTauri()) return
		if (hydratedTableTabs[tab.id]) return
		if (table.columns.length > 0) {
			hydratedTableTabs = { ...hydratedTableTabs, [tab.id]: true }
			return
		}
		if (tableLoading) return
		hydratedTableTabs = { ...hydratedTableTabs, [tab.id]: true }
		void openTable(table)
	})

	function onSelectConnection(id: string) {
		sideMode = 'explorer'
		workspaceSession.setActiveConnection(id)
	}

	function onInspectorFieldChange(column: string, value: unknown) {
		if (rowSelectionIndex === null || !gridTable) return
		const i = rowSelectionIndex
		const current = gridTable.rows[i] ?? {}
		gridTable.rows[i] = { ...current, [column]: value }
		rowSyncTick += 1
	}

	function onGridBaselineChange() {
		baselineEpoch += 1
	}

	function onSelectGridRow(index: number) {
		const tab = workspaceSession.openTabs.find(
			(t) => t.id === workspaceSession.activeTabId,
		)
		if (!tab || tab.kind !== 'table' || !tab.tableId) return
		workspaceSession.selectRow(tab.connectionId, tab.tableId, index)
	}

	function shortcutNewQuery() {
		const id = workspaceSession.activeConnectionId
		if (!id) return
		sideMode = 'explorer'
		workspaceSession.openNewQuery(id)
	}

	function shortcutCloseTab() {
		const id = workspaceSession.activeTabId
		if (!id) return
		requestCloseTab(id)
	}

	function shortcutSave() {
		const tab = workspaceSession.activeTab
		if (!tab) return
		if (tab.kind === 'query') {
			void saveActiveQuery()
			return
		}
		if (tab.kind === 'table') {
			window.dispatchEvent(new Event('mystique:save-table'))
		}
	}

	async function reloadGridTable(opts?: {
		offset?: number
		filters?: TablePreviewFilter[]
	}) {
		if (!gridTable) return
		if (activeTabId) {
			const { [activeTabId]: _done, ...rest } = hydratedTableTabs
			hydratedTableTabs = rest
		}
		await openTable(gridTable, opts?.offset ?? 0, opts?.filters ?? [])
	}

	onMount(() =>
		mountWorkspaceShortcuts({
			onNewQuery: shortcutNewQuery,
			onCloseTab: shortcutCloseTab,
			onSaveQuery: shortcutSave,
			isTauri: isTauri(),
		}),
	)
</script>

<div
	class="flex h-screen w-screen overflow-hidden bg-app"
	style={activeSession
		? `--connection-color: ${colorHex(activeSession.profile.colorId)}`
		: undefined}
	data-env={activeSession?.profile.environment ?? undefined}
>
	<SessionsRail
		{sessions}
		{activeId}
		mode={sideMode}
		onselect={onSelectConnection}
		ondisconnect={handleDisconnect}
		onrefresh={(id) => void refreshConnection(id)}
		onnewquery={openNewQueryOn}
		onopen={() => {
			pickerError = null
			pickerOpen = true
		}}
		onmode={setSideMode}
	/>

	<WorkspaceSidePane
		{sideMode}
		{activeSession}
		{databases}
		{currentDatabaseName}
		activeTableId={activeTableIdForExplorer}
		{refreshing}
		{modelLoading}
		{queryHistory}
		{savedQueries}
		{selectedQueryId}
		{selectedHistoryId}
		onselectquery={onSelectSavedQuery}
		onselecthistory={onSelectHistory}
		onselectdatabase={selectDatabase}
		onrefresh={refreshDatabases}
		oncreatedatabase={createDatabase}
		onopen={openTable}
		onopenmodel={() => void openModelDiagram(false)}
		onsavedaction={(action, id) => void onSavedQueryAction(action, id)}
	/>

	<WorkspaceMainPane
		{sideMode}
		{activeId}
		{activeSession}
		{connectionTabs}
		{activeTabId}
		{activeTab}
		{currentDatabaseName}
		{catalogError}
		{modelError}
		{modelLoading}
		{activeModel}
		{gridTable}
		{tableLoading}
		{rowSelectionIndex}
		{rowSyncTick}
		{baselineEpoch}
		{inspectorData}
		onhistory={(item) => prependHistoryItem(item)}
		onbaselinechange={onGridBaselineChange}
		onselectrow={onSelectGridRow}
		onopenmodelretry={() => void openModelDiagram(true)}
		oninspectorchange={onInspectorFieldChange}
		onsavequery={() => void saveActiveQuery()}
		querysaving={querySaving}
		onclosetab={requestCloseTab}
		onreloadtable={reloadGridTable}
		onreconnect={reconnectActiveSession}
	/>

	{#if pickerOpen}
		<OpenConnectionModal
			openIds={openConnectionIds}
			busy={pickerBusy}
			error={pickerError}
			onclose={() => {
				if (pickerBusy) return
				pickerOpen = false
				pickerError = null
			}}
			onpick={(profile) => void connectAdditional(profile)}
		/>
	{/if}

	<UnsavedCloseModal
		open={!!pendingCloseTab && !saveNameModalOpen}
		tabTitle={pendingCloseTab?.title ?? 'Tab'}
		tabKind={pendingCloseTab?.kind ?? 'query'}
		busy={closePromptBusy || querySaving}
		onsave={() => void confirmSaveAndClose()}
		ondiscard={() => void confirmDiscardAndClose()}
		oncancel={cancelCloseTab}
	/>

	<SaveQueryModal
		open={saveNameModalOpen}
		initialName={saveNameInitial}
		busy={querySaving}
		error={saveNameError}
		onconfirm={(name) => void onSaveNameConfirm(name)}
		oncancel={() => {
			if (querySaving) return
			saveThenClose = false
			closeSaveNameModal(false)
		}}
	/>

	<SaveQueryModal
		open={renameModalOpen}
		initialName={renameInitial}
		busy={renameBusy}
		error={renameError}
		title="Rename query"
		description="Update the name for this saved query."
		confirmLabel="Rename"
		onconfirm={(name) => void confirmRenameSavedQuery(name)}
		oncancel={() => {
			if (renameBusy) return
			renameModalOpen = false
			renameQueryId = null
			renameError = null
		}}
	/>

	{#if deleteModalOpen}
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-4"
			role="presentation"
			onclick={(e) => {
				if (e.target === e.currentTarget && !deleteBusy) {
					deleteModalOpen = false
					deleteQueryId = null
				}
			}}
			onkeydown={(e) => {
				if (e.key === 'Escape' && !deleteBusy) {
					deleteModalOpen = false
					deleteQueryId = null
				}
			}}
		>
			<div
				class="w-full max-w-md rounded-lg border border-line bg-elevated p-4 shadow-lg"
				role="dialog"
				aria-modal="true"
				aria-labelledby="delete-query-title"
			>
				<h2 id="delete-query-title" class="m-0 text-body font-semibold text-fg">
					Delete query
				</h2>
				<p class="mt-2 mb-0 text-label leading-relaxed text-fg-secondary">
					Delete “{deleteQueryName}”? This cannot be undone. Open tabs linked to
					this query will be closed.
				</p>
				<div class="mt-4 flex flex-wrap items-center justify-end gap-2">
					<button
						type="button"
						class="inline-flex h-control-sm items-center rounded-md border border-line bg-transparent px-2.5 text-label text-fg hover:bg-hover"
						disabled={deleteBusy}
						onclick={() => {
							deleteModalOpen = false
							deleteQueryId = null
						}}
					>
						Cancel
					</button>
					<button
						type="button"
						class="inline-flex h-control-sm items-center rounded-md border border-danger/40 bg-danger/15 px-2.5 text-label font-medium text-danger hover:bg-danger/25"
						disabled={deleteBusy}
						onclick={() => void confirmDeleteSavedQuery()}
					>
						{deleteBusy ? 'Deleting…' : 'Delete'}
					</button>
				</div>
			</div>
		</div>
	{/if}
</div>
