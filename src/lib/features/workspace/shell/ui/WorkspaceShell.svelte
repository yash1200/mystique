<script lang="ts">
	import { goto } from '$app/navigation'
	import { resolve } from '$app/paths'
	import type {
		CatalogObjectKind,
		ColumnReference,
		ConnectionProfile,
		DatabaseMeta,
		ImportEncoding,
		ImportFormat,
		TableAdminOp,
		TableMeta,
		TablePreviewFilter,
		TablePreviewSort,
	} from '$core/contracts'
	import {
		applyEngineTableAdmin,
		closeEngineSession,
		describeEngineObjectStructure,
		ensureEngineSession,
		exportEngineObject,
		importEngineObject,
		isTauri,
		openEngineSession,
		previewEngineImport,
		testEngineConnection,
	} from '$core/ipc'
	import { Button, Modal } from '$core/ui'
	import { setAppTitle } from '$core/util'
	import { modelTabTitle, resolveCapabilities } from '$engines'
	import {
		colorHex,
		connectionProfiles,
		draftFromProfile,
		pluginConfigForSession,
	} from '$features/connections'
	import {
		applyPreviewStats,
		catalogColumnsCache,
		loadLiveCatalog,
		loadTablePreview,
		loadTablePreviewStats,
		mergeCatalogKeepCached,
		mergeCatalogReplaceDatabase,
		preferDatabase,
	} from '$features/workspace/catalog'
	import { loadLiveErDiagram } from '$features/workspace/er'
	import type { TableExplorerAction } from '$features/workspace/explorer'
	import {
		ExportDataModal,
		ImportDataModal,
		ImportPreviewModal,
		type ExportDataSpec,
		type ImportFileSpec,
		type ImportRunSpec,
	} from '$features/workspace/grid'
	import type {
		QueryHistoryEntry,
		QueryHistoryItem,
	} from '$features/workspace/query'
	import {
		SaveQueryModal,
		deleteSavedQuery,
		historyItemToEntry,
		listQueryHistory,
		listRecentQueryHistory,
		listSavedQueries,
		queryLanguageFor,
		saveQuery,
		toListSavedQuery,
	} from '$features/workspace/query'
	import { onDestroy, onMount } from 'svelte'
	import {
		engineBindsSessionToDatabase,
		isSessionMissingError,
		modelCacheKey,
		preferredDbForProfile,
	} from '../session-utils'
	import { tableTabId, workspaceSession } from '../session.svelte'
	import { mountWorkspaceShortcuts } from '../shortcuts'
	import { workspaceStudio } from '../studio.svelte'
	import { resolveTablePreview } from '../types'
	import OpenConnectionModal from './OpenConnectionModal.svelte'
	import SessionsRail, { type SideNavMode } from './SessionsRail.svelte'
	import UnsavedCloseModal from './UnsavedCloseModal.svelte'
	import WorkspaceMainPane from './WorkspaceMainPane.svelte'
	import WorkspaceSidePane from './WorkspaceSidePane.svelte'

	const sessions = $derived(workspaceSession.sessions)
	const activeId = $derived(workspaceSession.activeConnectionId)
	const activeSession = $derived(workspaceSession.activeSession)
	const openTabs = $derived(workspaceSession.openTabs)
	const activeTabId = $derived(workspaceSession.activeTabId)
	const selectedRow = $derived(workspaceSession.selectedRow)

	// OS window title: "Mystique · {connection}" (system title bar only).
	$effect(() => {
		void setAppTitle(activeSession?.profile.name ?? null)
	})
	onDestroy(() => {
		void setAppTitle()
	})

	/** Monotonic token so overlapping history fetches don't clobber newer data. */
	let historyFetchGen = 0

	/** Open / re-open a pooled engine session for a profile (fixes stale old connections). */
	async function reopenEngineSession(
		profile: ConnectionProfile,
		database?: string | null,
	): Promise<string> {
		const pluginConfig = pluginConfigForSession(profile)
		const db = database?.trim()
		if (db) pluginConfig.database = db
		const password = profile.password?.trim() || null
		const request = {
			engine: String(profile.engine),
			pluginConfig,
			password,
			pool: { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 },
			sessionId: profile.id,
		}
		// MCP (and other tools) open sessions as profile.id — reuse unless we
		// must rebind to another database.
		const session = db
			? await openEngineSession(request)
			: await ensureEngineSession(request)
		workspaceSession.connect(profile, session.sessionId, session.capabilities)
		return session.sessionId
	}

	function sessionDatabase(
		connectionId: string,
		profile: ConnectionProfile,
	): string | null {
		const selected = workspaceStudio.selectedDb[connectionId]?.trim()
		if (selected) return selected
		return preferredDbForProfile(profile)
	}

	function systemDbNames(engine: string, connectionId?: string): string[] {
		const session = connectionId
			? sessions.find((s) => s.profile.id === connectionId)
			: activeSession
		return resolveCapabilities(engine, session?.capabilities).systemDatabases
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
		workspaceStudio.catalogError = null
		let sessionId = engineSessionId
		try {
			const live = await loadLiveCatalog(sessionId, engine, {
				preferredDatabase,
				includeObjects,
			})
			workspaceStudio.catalogs = {
				...workspaceStudio.catalogs,
				[connectionId]: mergeCatalogKeepCached(
					workspaceStudio.catalogs[connectionId],
					live,
				),
			}
			workspaceStudio.liveCatalogKey = {
				...workspaceStudio.liveCatalogKey,
				[connectionId]: sessionId,
			}
			const names = live.map((d) => d.name)
			const pick = preferDatabase(
				names,
				preferredDatabase,
				systemDbNames(engine, connectionId),
			)
			const current = workspaceStudio.selectedDb[connectionId]
			if (current && names.includes(current)) {
				// Keep the database the user switched to.
			} else if (pick) {
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
						includeObjects,
					})
					workspaceStudio.catalogs = {
						...workspaceStudio.catalogs,
						[connectionId]: mergeCatalogKeepCached(
							workspaceStudio.catalogs[connectionId],
							live,
						),
					}
					workspaceStudio.liveCatalogKey = {
						...workspaceStudio.liveCatalogKey,
						[connectionId]: sessionId,
					}
					const names = live.map((d) => d.name)
					const pick = preferDatabase(
						names,
						preferredDatabase,
						systemDbNames(engine, connectionId),
					)
					const current = workspaceStudio.selectedDb[connectionId]
					if (current && names.includes(current)) {
						// Keep the database the user switched to.
					} else if (pick) {
						workspaceStudio.selectedDb = {
							...workspaceStudio.selectedDb,
							[connectionId]: pick,
						}
					}
					workspaceStudio.catalogError = null
					return
				} catch (retryErr) {
					workspaceStudio.catalogError =
						retryErr instanceof Error ? retryErr.message : String(retryErr)
				}
			} else {
				workspaceStudio.catalogError =
					e instanceof Error ? e.message : String(e)
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

	$effect(() => {
		const session = activeSession
		if (!session) return
		const id = session.profile.id
		const engine = String(session.profile.engine)
		const engineSessionId = session.engineSessionId ?? null
		const preferred = sessionDatabase(id, session.profile)

		// Catalog only — do not auto-open a Query tab (empty workspace is intentional).
		if (!isTauri()) return

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
					workspaceStudio.catalogError =
						e instanceof Error ? e.message : String(e)
					return
				}
			}
			await ensureLiveCatalog(id, engine, sid, preferred)
		})()
	})

	const databases = $derived(
		activeId ? (workspaceStudio.catalogs[activeId] ?? []) : [],
	)

	const objectKindById = $derived.by(() => {
		const map: Record<string, CatalogObjectKind> = {}
		for (const db of databases) {
			for (const t of db.tables) map[t.id] = t.kind
		}
		return map
	})

	const currentDatabaseName = $derived(
		activeId
			? (workspaceStudio.selectedDb[activeId] ?? databases[0]?.name ?? '')
			: '',
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
		const cat = workspaceStudio.catalogs[activeTab.connectionId]
		if (!cat) return null
		for (const d of cat) {
			const t = d.tables.find((x) => x.id === activeTab.tableId)
			if (t) return t
		}
		return null
	})

	const activeTableIdForExplorer = $derived(
		activeTab &&
			(activeTab.kind === 'table' || activeTab.kind === 'structure') &&
			activeTab.connectionId === activeId
			? (activeTab.tableId ?? null)
			: null,
	)

	const activeStructure = $derived.by(() => {
		if (
			!activeId ||
			!activeTab ||
			activeTab.kind !== 'structure' ||
			!activeTab.tableId
		) {
			return null
		}
		return (
			workspaceStudio.liveStructures[
				structureCacheKey(activeId, activeTab.tableId)
			] ?? null
		)
	})

	const structureTableMeta = $derived.by(() => {
		if (
			!activeTab ||
			activeTab.kind !== 'structure' ||
			!activeTab.tableId ||
			!activeId
		) {
			return null
		}
		const cat = workspaceStudio.catalogs[activeId] ?? []
		for (const d of cat) {
			const t = d.tables.find((x) => x.id === activeTab.tableId)
			if (t) return t
		}
		return null
	})

	// Reload structure when a structure tab is focused without cached data.
	$effect(() => {
		if (
			!activeId ||
			!activeTab ||
			activeTab.kind !== 'structure' ||
			!activeTab.tableId ||
			workspaceStudio.structureLoading ||
			workspaceStudio.structureError
		) {
			return
		}
		const key = structureCacheKey(activeId, activeTab.tableId)
		if (workspaceStudio.liveStructures[key]?.data) return
		const table = structureTableMeta
		if (!table) return
		void openObjectStructure(table)
	})

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
		if (workspaceStudio.sideMode !== 'explorer') return null
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

	async function refreshSavedQueries(
		connectionId: string | null,
		force = false,
	) {
		if (!connectionId || !isTauri()) {
			workspaceStudio.savedQueries = []
			workspaceStudio.savedQueriesFor = null
			return
		}
		if (!force && workspaceStudio.savedQueriesFor === connectionId) return
		try {
			const rows = await listSavedQueries(connectionId)
			workspaceStudio.savedQueries = rows.map(toListSavedQuery)
			workspaceStudio.savedQueriesFor = connectionId
		} catch (e) {
			console.warn('[mystique] list saved queries:', e)
			workspaceStudio.savedQueries = []
			workspaceStudio.savedQueriesFor = connectionId
		}
	}

	/** Tab id waiting on the unsaved-close modal. */
	let pendingCloseTabId = $state<string | null>(null)
	let closePromptBusy = $state(false)
	/** Remaining tab ids from Close others / all / left / right. */
	let closeQueue = $state<string[]>([])

	const pendingCloseTab = $derived(
		pendingCloseTabId
			? (workspaceSession.openTabs.find((t) => t.id === pendingCloseTabId) ??
					null)
			: null,
	)

	function requestCloseTab(tabId: string) {
		enqueueCloseTabs([tabId])
	}

	function enqueueCloseTabs(ids: string[]) {
		const open = new Set(workspaceSession.openTabs.map((t) => t.id))
		const queued = new Set(closeQueue)
		const next = ids.filter((id) => open.has(id) && !queued.has(id))
		if (!next.length) return
		closeQueue = [...closeQueue, ...next]
		drainCloseQueue()
	}

	function drainCloseQueue() {
		if (pendingCloseTabId) return
		while (closeQueue.length) {
			const id = closeQueue[0]!
			const tab = workspaceSession.openTabs.find((t) => t.id === id)
			if (!tab) {
				closeQueue = closeQueue.slice(1)
				continue
			}
			if (tab.dirty) {
				// Focus the tab so Save/Discard apply to the visible content.
				workspaceSession.setActiveTab(id)
				pendingCloseTabId = id
				return
			}
			workspaceSession.closeTab(id)
			closeQueue = closeQueue.slice(1)
		}
	}

	function finishQueuedClose(tabId: string) {
		pendingCloseTabId = null
		closeQueue = closeQueue.filter((id) => id !== tabId)
		drainCloseQueue()
	}

	function cancelCloseTab() {
		if (closePromptBusy) return
		pendingCloseTabId = null
		closeQueue = []
	}

	/** In-app save-name modal (window.prompt is broken in Tauri webviews). */
	let saveNameModalOpen = $state(false)
	let saveNameTabId = $state<string | null>(null)
	let saveNameInitial = $state('Query')
	let saveNameError = $state<string | null>(null)
	/** Resolves the pending saveQueryTab() promise after modal confirm/cancel. */
	let saveNameResolver: ((ok: boolean) => void) | null = null
	/** When true, close the tab after a successful save from the name modal. */
	let saveThenClose = $state(false)

	function needsNamePrompt(title: string): boolean {
		const name = title?.trim() || ''
		if (!name) return true
		if (/^untitled query(?:\s+\d+)?$/i.test(name)) return true
		if (/^query(?:\s+\d+)?$/i.test(name)) return true
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
		const language = queryLanguageFor(
			String(session.profile.engine),
			session.capabilities,
		)
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
		await refreshSavedQueries(tab.connectionId, true)
		workspaceStudio.selectedQueryId = saved.id
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
		workspaceStudio.querySaving = true
		saveNameError = null
		try {
			await persistQueryTab(tabId, name)
			const shouldClose = saveThenClose
			saveThenClose = false
			closeSaveNameModal(true)
			if (shouldClose) {
				workspaceSession.closeTab(tabId)
				finishQueuedClose(tabId)
			}
		} catch (e) {
			saveNameError = e instanceof Error ? e.message : String(e)
			console.warn('[mystique] save query:', e)
		} finally {
			workspaceStudio.querySaving = false
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
				tab.title && !needsNamePrompt(tab.title) ? tab.title : 'Query'
			saveNameError = null
			saveThenClose = !!opts?.thenClose
			saveNameModalOpen = true
			return await new Promise<boolean>((resolve) => {
				saveNameResolver = resolve
			})
		}

		workspaceStudio.querySaving = true
		try {
			await persistQueryTab(tabId, tab.title.trim())
			return true
		} catch (e) {
			console.warn('[mystique] save query:', e)
			alert(e instanceof Error ? e.message : String(e))
			return false
		} finally {
			workspaceStudio.querySaving = false
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
					workspaceSession.closeTab(tab.id)
				}
				finishQueuedClose(tab.id)
			} else {
				// Table / model: keep in-memory edits, clear dirty, then close.
				workspaceSession.setTabDirty(tab.id, false)
				const id = tab.id
				workspaceSession.closeTab(id)
				finishQueuedClose(id)
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
				const cat = workspaceStudio.catalogs[tab.connectionId]
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
							const list = workspaceStudio.catalogs[tab.connectionId] ?? []
							workspaceStudio.catalogs = {
								...workspaceStudio.catalogs,
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
				workspaceStudio.baselineEpoch += 1
			} else {
				workspaceSession.setTabDirty(tab.id, false)
			}
			const id = tab.id
			workspaceSession.closeTab(id)
			finishQueuedClose(id)
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
		workspaceStudio.queryHistory = [
			entry,
			...workspaceStudio.queryHistory.filter((h) => h.id !== entry.id),
		].slice(0, 150)
		workspaceStudio.historyError = null
		// Highlight the new run in the list; user opens it explicitly if desired.
		workspaceStudio.selectedHistoryId = entry.id
	}

	async function refreshHistory(opts?: {
		preferSelectLatest?: boolean
		force?: boolean
	}) {
		const scope = activeId ?? null
		if (
			!opts?.force &&
			workspaceStudio.historyFor === scope &&
			(workspaceStudio.queryHistory.length > 0 ||
				workspaceStudio.historyError == null)
		) {
			if (opts?.preferSelectLatest && workspaceStudio.queryHistory[0]) {
				workspaceStudio.selectedHistoryId = workspaceStudio.queryHistory[0].id
			}
			return
		}
		if (!isTauri()) {
			workspaceStudio.queryHistory = []
			workspaceStudio.historyFor = scope
			return
		}
		const gen = ++historyFetchGen
		workspaceStudio.historyLoading = true
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
			workspaceStudio.queryHistory = items.map((it) => entryFromItem(it))
			workspaceStudio.historyFor = scope
			workspaceStudio.historyError = null
			// Keep list highlight only — do not auto-open runs in the editor.
			if (
				workspaceStudio.selectedHistoryId &&
				!workspaceStudio.queryHistory.some(
					(h) => h.id === workspaceStudio.selectedHistoryId,
				)
			) {
				workspaceStudio.selectedHistoryId = null
			}
			if (opts?.preferSelectLatest && workspaceStudio.queryHistory[0]) {
				workspaceStudio.selectedHistoryId = workspaceStudio.queryHistory[0].id
			}
		} catch (e) {
			if (gen !== historyFetchGen) return
			// Keep existing rows; surface the error instead of wiping the list.
			workspaceStudio.historyError = e instanceof Error ? e.message : String(e)
			console.warn(
				'[mystique] refreshHistory failed:',
				workspaceStudio.historyError,
			)
		} finally {
			if (gen === historyFetchGen) workspaceStudio.historyLoading = false
		}
	}

	$effect(() => {
		// Reload history when connection changes or history pane is focused.
		const _id = activeId
		const mode = workspaceStudio.sideMode
		if (mode === 'history' || _id) {
			void refreshHistory({ preferSelectLatest: mode === 'history' })
		}
	})

	/** Open SQL in a new query tab (history / external). */
	function openSqlInEditor(sql: string, title?: string) {
		if (!activeId) return
		workspaceSession.openNewQueryWithText(activeId, sql, title)
	}

	function structureCacheKey(connectionId: string, tableId: string): string {
		return `${connectionId}::${tableId}`
	}

	async function openObjectStructure(table: TableMeta, force = false) {
		if (!activeId || !activeSession) return
		workspaceStudio.sideMode = 'explorer'
		const title = table.name
		workspaceSession.openStructure(activeId, table.id, title)
		const key = structureCacheKey(activeId, table.id)
		if (!force && workspaceStudio.liveStructures[key]?.data?.sections?.length) {
			workspaceStudio.structureError = null
			return
		}
		if (!isTauri()) {
			workspaceStudio.structureError =
				'Object structure requires Tauri. Run `npm run tauri dev` and Connect.'
			return
		}
		workspaceStudio.structureLoading = true
		workspaceStudio.structureError = null
		try {
			const loaded = await withLiveSession(activeId, (sid) =>
				describeEngineObjectStructure({
					sessionId: sid,
					table: table.name,
					schema: table.schema || null,
					database: table.database || currentDatabaseName || null,
					kind: table.kind,
				}),
			)
			workspaceStudio.liveStructures = {
				...workspaceStudio.liveStructures,
				[key]: { data: loaded, rev: Date.now() },
			}
			if (loaded.title) {
				workspaceSession.openStructure(activeId, table.id, table.name)
			}
		} catch (e) {
			workspaceStudio.structureError =
				e instanceof Error ? e.message : String(e)
		} finally {
			workspaceStudio.structureLoading = false
		}
	}

	async function onTableExplorerAction(
		action: TableExplorerAction,
		table: TableMeta,
		extra?: string,
	) {
		if (!activeId || !activeSession) return
		if (action === 'open') {
			void openTable(table)
			return
		}
		if (action === 'erd') {
			void openModelDiagram(false, { table })
			return
		}
		if (action === 'structure') {
			void openObjectStructure(table)
			return
		}
		if (action === 'export') {
			workspaceStudio.exportTable = table
			workspaceStudio.exportError = null
			return
		}
		if (action === 'import') {
			workspaceStudio.importTable = table
			workspaceStudio.importError = null
			workspaceStudio.importPreview = null
			workspaceStudio.importPath = ''
			return
		}
		if (!isTauri()) {
			workspaceStudio.catalogError =
				'Live catalog actions require the desktop app.'
			return
		}

		const newName = extra?.trim() || null
		if ((action === 'rename' || action === 'duplicate') && !newName) return

		const op: TableAdminOp = action === 'create' ? 'createStatement' : action

		try {
			workspaceStudio.catalogError = null
			const result = await withLiveSession(activeId, (sid) =>
				applyEngineTableAdmin({
					sessionId: sid,
					op,
					name: table.name,
					schema: table.schema || null,
					database: table.database || currentDatabaseName,
					kind: table.kind,
					newName,
					profileReadOnly: !!activeSession.profile.readOnly,
				}),
			)

			// View Create is the only action that opens a query tab.
			if (action === 'create') {
				const sql = (result.statement ?? '').trim()
				if (!sql) {
					workspaceStudio.catalogError =
						'Engine returned an empty create statement.'
					return
				}
				openSqlInEditor(
					sql.endsWith(';') || sql.includes('createCollection')
						? `${sql}\n`
						: `${sql};\n`,
					`${table.name} · create`,
				)
				return
			}

			// Rename / drop / truncate / duplicate run in the plugin.
			const tabId = `${activeId}::table::${table.id}`
			const structureTabId = `${activeId}::structure::${table.id}`
			const tableOpen = workspaceSession.openTabs.some((t) => t.id === tabId)
			const structureOpen = workspaceSession.openTabs.some(
				(t) => t.id === structureTabId,
			)
			if (action === 'drop' || action === 'rename') {
				if (tableOpen) workspaceSession.closeTab(tabId)
				if (structureOpen) workspaceSession.closeTab(structureTabId)
				const sk = structureCacheKey(activeId, table.id)
				if (workspaceStudio.liveStructures[sk]) {
					const { [sk]: _drop, ...rest } = workspaceStudio.liveStructures
					workspaceStudio.liveStructures = rest
				}
			} else if (action === 'truncate' && tableOpen) {
				void openTable(table)
			}
			await refreshCurrentTables()
		} catch (e) {
			workspaceStudio.catalogError = e instanceof Error ? e.message : String(e)
		}
	}

	function closeImport() {
		if (workspaceStudio.importBusy) return
		workspaceStudio.importTable = null
		workspaceStudio.importPreview = null
		workspaceStudio.importError = null
		workspaceStudio.importPath = ''
	}

	async function loadImportPreview(
		table: TableMeta,
		path: string,
		format: ImportFormat,
		encoding: ImportEncoding,
	) {
		if (!activeId) return
		workspaceStudio.importBusy = true
		workspaceStudio.importError = null
		try {
			workspaceStudio.importPreview = await withLiveSession(activeId, (sid) =>
				previewEngineImport({
					sessionId: sid,
					name: table.name,
					schema: table.schema || null,
					database: table.database || currentDatabaseName,
					kind: table.kind,
					path,
					format,
					encoding,
				}),
			)
			workspaceStudio.importPath = path
			workspaceStudio.importFormat = format
		} catch (e) {
			workspaceStudio.importError = e instanceof Error ? e.message : String(e)
		} finally {
			workspaceStudio.importBusy = false
		}
	}

	async function confirmImportFile(spec: ImportFileSpec) {
		if (!workspaceStudio.importTable) return
		if (!isTauri()) {
			workspaceStudio.importError = 'Import requires the desktop app.'
			return
		}
		await loadImportPreview(
			workspaceStudio.importTable,
			spec.path,
			spec.format,
			'utf8',
		)
	}

	async function confirmImportRun(spec: ImportRunSpec) {
		if (
			!workspaceStudio.importTable ||
			!activeId ||
			!workspaceStudio.importPath
		)
			return
		const table = workspaceStudio.importTable
		workspaceStudio.importBusy = true
		workspaceStudio.importError = null
		try {
			await withLiveSession(activeId, (sid) =>
				importEngineObject({
					sessionId: sid,
					name: table.name,
					schema: table.schema || null,
					database: table.database || currentDatabaseName,
					kind: table.kind,
					path: workspaceStudio.importPath,
					format: workspaceStudio.importFormat,
					encoding: spec.encoding,
					mapping: spec.mapping,
					onDuplicate: spec.onDuplicate,
					profileReadOnly: !!activeSession?.profile.readOnly,
				}),
			)
			workspaceStudio.importTable = null
			workspaceStudio.importPreview = null
			workspaceStudio.importPath = ''
			const tabId = `${activeId}::table::${table.id}`
			if (workspaceSession.openTabs.some((t) => t.id === tabId)) {
				void openTable(table)
			}
		} catch (e) {
			workspaceStudio.importError = e instanceof Error ? e.message : String(e)
		} finally {
			workspaceStudio.importBusy = false
		}
	}

	async function confirmExport(spec: ExportDataSpec) {
		if (!workspaceStudio.exportTable || !activeId) return
		if (!isTauri()) {
			workspaceStudio.exportError = 'Export requires the desktop app.'
			return
		}
		const table = workspaceStudio.exportTable
		workspaceStudio.exportBusy = true
		workspaceStudio.exportError = null
		try {
			const { join } = await import('@tauri-apps/api/path')
			const path = await join(spec.folder, spec.fileName)
			await withLiveSession(activeId, (sid) =>
				exportEngineObject({
					sessionId: sid,
					name: table.name,
					schema: table.schema || null,
					database: table.database || currentDatabaseName,
					kind: table.kind,
					format: spec.format,
					path,
				}),
			)
			workspaceStudio.exportTable = null
		} catch (e) {
			workspaceStudio.exportError = e instanceof Error ? e.message : String(e)
		} finally {
			workspaceStudio.exportBusy = false
		}
	}

	function onSelectHistory(id: string) {
		workspaceStudio.selectedHistoryId = id
		const h = workspaceStudio.queryHistory.find((x) => x.id === id)
		if (!h || !activeId) return
		// Open the run directly in the query editor (no preview pane).
		openSqlInEditor(h.sql)
	}

	async function openTable(
		table: TableMeta,
		offset = 0,
		filters: TablePreviewFilter[] = [],
		sort: TablePreviewSort | null = null,
		codeFilter: string | null = null,
		replacePreview = false,
	) {
		if (!activeId || !activeSession) return
		workspaceStudio.sideMode = 'explorer'
		const tabId = tableTabId(activeId, table.id)
		const existing = workspaceSession.openTabs.find((t) => t.id === tabId)
		const preview = resolveTablePreview(existing?.preview, {
			offset,
			filters,
			sort,
			codeFilter,
			replace: replacePreview,
		})
		workspaceSession.openTable(activeId, table.id, table.name)
		workspaceSession.setTablePreview(tabId, preview)

		if (!isTauri()) return

		const epoch = (previewEpochByTab[tabId] ?? 0) + 1
		previewEpochByTab = { ...previewEpochByTab, [tabId]: epoch }
		const stillThis = () => previewEpochByTab[tabId] === epoch

		workspaceStudio.loadingTabId = tabId
		workspaceStudio.catalogError = null
		try {
			const engine = String(activeSession.profile.engine)
			const loaded = await withLiveSession(activeId, (sid) =>
				loadTablePreview(
					sid,
					engine,
					table,
					100,
					preview.offset,
					preview.filters,
					preview.sort,
					preview.codeFilter,
				),
			)
			if (!stillThis()) return
			patchCatalogTable(activeId, table.id, loaded)
			// Only the visible grid should adopt this page. A background
			// finish for another tab must not reset the active grid's baseline.
			if (workspaceSession.activeTabId === tabId) {
				workspaceStudio.tablePreviewTick += 1
			}

			void withLiveSession(activeId, (sid) =>
				loadTablePreviewStats(sid, table, preview.filters, preview.codeFilter),
			)
				.then((stats) => {
					if (!stillThis()) return
					const current = findCatalogTable(activeId, table.id)
					if (!current) return
					patchCatalogTable(
						activeId,
						table.id,
						applyPreviewStats(current, stats),
					)
				})
				.catch(() => {
					/* pager stays approximate until the next refresh */
				})
		} catch (e) {
			if (stillThis() && workspaceSession.activeTabId === tabId) {
				workspaceStudio.catalogError =
					e instanceof Error ? e.message : String(e)
			}
		} finally {
			if (stillThis() && workspaceStudio.loadingTabId === tabId) {
				workspaceStudio.loadingTabId = null
			}
		}
	}

	function findCatalogTable(
		connectionId: string,
		tableId: string,
	): TableMeta | null {
		const list = workspaceStudio.catalogs[connectionId] ?? []
		for (const db of list) {
			const hit = db.tables.find((t) => t.id === tableId)
			if (hit) return hit
		}
		return null
	}

	function patchCatalogTable(
		connectionId: string,
		tableId: string,
		next: TableMeta,
	) {
		const list = workspaceStudio.catalogs[connectionId] ?? []
		workspaceStudio.catalogs = {
			...workspaceStudio.catalogs,
			[connectionId]: list.map((db) => ({
				...db,
				tables: db.tables.map((t) => (t.id === tableId ? next : t)),
			})),
		}
	}

	/** Resolve a plugin FK target to a catalog TableMeta (same connection). */
	function findReferencedTable(ref: ColumnReference): TableMeta | null {
		if (!activeId) return null
		const list = workspaceStudio.catalogs[activeId] ?? []
		const refDb = (ref.database ?? '').trim()
		const refSchema = (ref.schema ?? '').trim()
		const refTable = ref.table.trim()
		const matches: TableMeta[] = []
		for (const db of list) {
			for (const t of db.tables) {
				if (t.name !== refTable) continue
				if (refSchema && t.schema && t.schema !== refSchema) continue
				if (refDb && t.database && t.database !== refDb) continue
				matches.push(t)
			}
		}
		if (matches.length === 0) return null
		// Prefer exact schema match, then current database, then first hit.
		const curDb = currentDatabaseName
		return (
			matches.find((t) => refSchema && t.schema === refSchema) ??
			matches.find((t) => t.database === curDb) ??
			matches[0] ??
			null
		)
	}

	async function openForeignKeyReference(ref: ColumnReference, value: unknown) {
		if (value === null || value === undefined) return
		const target = findReferencedTable(ref)
		if (!target) {
			workspaceStudio.catalogError = `Referenced table “${ref.schema ? `${ref.schema}.` : ''}${ref.table}” is not in the explorer catalog.`
			return
		}
		// Switch database session when the FK points at another DB (MySQL).
		const targetDb = (target.database || ref.database || '').trim()
		if (
			targetDb &&
			currentDatabaseName &&
			targetDb !== currentDatabaseName &&
			activeSession
		) {
			const engine = String(activeSession.profile.engine)
			if (engineBindsSessionToDatabase(engine, activeSession.capabilities)) {
				try {
					await selectDatabase(targetDb)
				} catch {
					/* selectDatabase sets workspaceStudio.catalogError */
				}
			} else {
				workspaceStudio.selectedDb = {
					...workspaceStudio.selectedDb,
					[activeId!]: targetDb,
				}
			}
		}
		const filterValue =
			typeof value === 'string' || typeof value === 'number'
				? String(value)
				: JSON.stringify(value)
		await openTable(target, 0, [
			{ column: ref.column, op: 'eq', value: filterValue },
		])
	}

	async function selectDatabase(name: string) {
		if (!activeId || !activeSession) return
		const next = name.trim()
		if (!next || workspaceStudio.selectedDb[activeId] === next) return
		const previous = workspaceStudio.selectedDb[activeId] ?? ''
		workspaceStudio.selectedDb = {
			...workspaceStudio.selectedDb,
			[activeId]: next,
		}

		const engine = String(activeSession.profile.engine)
		if (
			!isTauri() ||
			!engineBindsSessionToDatabase(engine, activeSession?.capabilities)
		)
			return

		workspaceStudio.refreshing = true
		workspaceStudio.catalogError = null
		try {
			const sid = await reopenEngineSession(activeSession.profile, next)
			await ensureLiveCatalog(activeId, engine, sid, next, true)
		} catch (e) {
			workspaceStudio.selectedDb = {
				...workspaceStudio.selectedDb,
				[activeId]: previous,
			}
			workspaceStudio.catalogError = e instanceof Error ? e.message : String(e)
			workspaceStudio.refreshing = false
		}
	}

	const modelTableMeta = $derived.by(() => {
		if (
			!activeTab ||
			activeTab.kind !== 'model' ||
			!activeTab.tableId ||
			!activeId
		) {
			return null
		}
		const cat = workspaceStudio.catalogs[activeId] ?? []
		for (const d of cat) {
			const t = d.tables.find((x) => x.id === activeTab.tableId)
			if (t) return t
		}
		return null
	})

	/** Open live ER for one table and the tables directly linked to it. */
	async function openModelDiagram(
		force = false,
		opts?: { database?: string | null; table?: TableMeta | null },
	) {
		if (!activeSession || !activeId) return
		const table = opts?.table ?? modelTableMeta
		if (!table) {
			workspaceStudio.modelError = 'Pick a table to open its ER diagram.'
			return
		}
		workspaceStudio.sideMode = 'explorer'
		const engine = String(activeSession.profile.engine)
		const database = (
			opts?.database ??
			table.database ??
			currentDatabaseName ??
			''
		).trim()
		const title = modelTabTitle(
			resolveCapabilities(engine, activeSession.capabilities),
			database,
			table.name,
		)

		const key = modelCacheKey(activeId, database, table.id)
		const alreadyOpen =
			activeTab?.kind === 'model' &&
			(activeTab.modelDatabase ?? '') === database &&
			(activeTab.tableId ?? '') === table.id
		workspaceSession.openModel(activeId, database, title, table.id)

		// Reuse cache when switching back to the tab; force rebuild on refresh.
		if (
			!force &&
			!alreadyOpen &&
			workspaceStudio.liveModels[key]?.diagram?.nodes?.length
		) {
			workspaceStudio.modelError = null
			return
		}

		if (!isTauri()) {
			workspaceStudio.modelError =
				'Live ER diagram requires Tauri. Run `npm run tauri dev` and Connect.'
			return
		}

		workspaceStudio.modelLoading = true
		workspaceStudio.modelError = null
		try {
			let sid = activeSession.engineSessionId
			if (!sid) {
				sid = await reopenEngineSession(
					activeSession.profile,
					sessionDatabase(activeId, activeSession.profile),
				)
			}
			const loaded = await loadLiveErDiagram(
				sid,
				engine,
				database || null,
				activeSession.capabilities,
				{ schema: table.schema, name: table.name },
			)
			workspaceStudio.liveModels = {
				...workspaceStudio.liveModels,
				[key]: {
					diagram: loaded.diagram,
					title: loaded.title,
					summary: loaded.summary,
					rev: Date.now(),
					focusNodeId: loaded.focusNodeId,
				},
			}
			workspaceSession.openModel(activeId, database, loaded.title, table.id)
		} catch (e) {
			// Session missing — reopen once
			if (isSessionMissingError(e)) {
				try {
					const sid = await reopenEngineSession(
						activeSession.profile,
						sessionDatabase(activeId, activeSession.profile),
					)
					const loaded = await loadLiveErDiagram(
						sid,
						engine,
						database || null,
						activeSession.capabilities,
						{ schema: table.schema, name: table.name },
					)
					workspaceStudio.liveModels = {
						...workspaceStudio.liveModels,
						[key]: {
							diagram: loaded.diagram,
							title: loaded.title,
							summary: loaded.summary,
							rev: Date.now(),
							focusNodeId: loaded.focusNodeId,
						},
					}
					workspaceSession.openModel(activeId, database, loaded.title, table.id)
					workspaceStudio.modelError = null
					return
				} catch (retryErr) {
					workspaceStudio.modelError =
						retryErr instanceof Error ? retryErr.message : String(retryErr)
					return
				}
			}
			workspaceStudio.modelError = e instanceof Error ? e.message : String(e)
		} finally {
			workspaceStudio.modelLoading = false
		}
	}

	const activeModel = $derived.by(() => {
		if (!activeTab || activeTab.kind !== 'model' || !activeId) return null
		const db = activeTab.modelDatabase ?? currentDatabaseName ?? ''
		return (
			workspaceStudio.liveModels[
				modelCacheKey(activeId, db, activeTab.tableId)
			] ?? null
		)
	})

	// Rebuild when a model tab is focused without a cached diagram
	// (reconnect / app reopen restores the tab but workspaceStudio.liveModels is empty).
	$effect(() => {
		if (
			!activeId ||
			!activeSession ||
			!activeTab ||
			activeTab.kind !== 'model' ||
			!activeTab.tableId ||
			workspaceStudio.modelLoading ||
			workspaceStudio.modelError
		) {
			return
		}
		const db = activeTab.modelDatabase ?? currentDatabaseName ?? ''
		if (
			workspaceStudio.liveModels[modelCacheKey(activeId, db, activeTab.tableId)]
				?.diagram
		) {
			return
		}
		const table = modelTableMeta
		if (!table) return
		void openModelDiagram(true, { database: db, table })
	})

	async function refreshDatabases() {
		if (!activeSession || !activeId) return
		const engine = String(activeSession.profile.engine)
		const preferred = sessionDatabase(activeId, activeSession.profile)

		if (!isTauri()) return

		workspaceStudio.refreshing = true
		workspaceStudio.catalogError = null
		try {
			const sid = await withLiveSession(
				activeId,
				async (sessionId) => sessionId,
			)
			await ensureLiveCatalog(activeId, engine, sid, preferred, true, false)
		} catch (e) {
			workspaceStudio.catalogError = e instanceof Error ? e.message : String(e)
			workspaceStudio.refreshing = false
		}
	}

	let refreshingTables = $state(false)

	async function refreshCurrentTables() {
		if (!activeSession || !activeId) return
		const engine = String(activeSession.profile.engine)
		const db =
			currentDatabaseName || sessionDatabase(activeId, activeSession.profile)

		if (!isTauri()) return

		refreshingTables = true
		workspaceStudio.catalogError = null
		try {
			const sid = await withLiveSession(
				activeId,
				async (sessionId) => sessionId,
			)
			const live = await loadLiveCatalog(sid, engine, {
				preferredDatabase: db,
				includeObjects: true,
			})
			workspaceStudio.catalogs = {
				...workspaceStudio.catalogs,
				[activeId]: mergeCatalogReplaceDatabase(
					workspaceStudio.catalogs[activeId],
					live,
					db || '',
				),
			}
		} catch (e) {
			workspaceStudio.catalogError = e instanceof Error ? e.message : String(e)
		} finally {
			refreshingTables = false
		}
	}

	function createDatabase(name: string) {
		if (!activeId) return
		const entry: DatabaseMeta = {
			name,
			meta: 'created',
			tables: [],
		}
		const list = [...(workspaceStudio.catalogs[activeId] ?? []), entry]
		workspaceStudio.catalogs = { ...workspaceStudio.catalogs, [activeId]: list }
		workspaceStudio.selectedDb = {
			...workspaceStudio.selectedDb,
			[activeId]: name,
		}
	}

	let createBusy = $state(false)

	async function createCatalogGroup(name: string) {
		if (!activeId || !activeSession) return
		// Mock catalog: group is empty until an object is added; explorer keeps localGroups.
		if (!isTauri()) {
			workspaceStudio.catalogError = null
			return
		}
		createBusy = true
		workspaceStudio.catalogError = null
		try {
			await withLiveSession(activeId, (sid) =>
				applyEngineTableAdmin({
					sessionId: sid,
					op: 'createGroup',
					name,
					database: currentDatabaseName,
					profileReadOnly: !!activeSession.profile.readOnly,
				}),
			)
			await refreshCurrentTables()
		} catch (e) {
			workspaceStudio.catalogError = e instanceof Error ? e.message : String(e)
		} finally {
			createBusy = false
		}
	}

	async function createCatalogObject(name: string, group?: string) {
		if (!activeId || !activeSession) return
		const caps = resolveCapabilities(
			String(activeSession.profile.engine),
			activeSession.capabilities,
		)
		const kind =
			caps.explorer.objectLabelSingular === 'collection'
				? 'collection'
				: 'table'
		if (!isTauri()) {
			const dbName = currentDatabaseName
			const schema = group || (kind === 'collection' ? '' : 'public')
			const id = schema ? `${dbName}.${schema}.${name}` : `${dbName}.${name}`
			const table: TableMeta = {
				id,
				name,
				schema,
				database: dbName,
				kind: kind as TableMeta['kind'],
				rowCount: 0,
				columns: [],
				rows: [],
			}
			const list = (workspaceStudio.catalogs[activeId] ?? []).map((d) => {
				if (d.name !== dbName) return d
				if (d.tables.some((t) => t.id === id)) return d
				return { ...d, tables: [...d.tables, table] }
			})
			workspaceStudio.catalogs = {
				...workspaceStudio.catalogs,
				[activeId]: list,
			}
			workspaceStudio.catalogError = null
			return
		}
		createBusy = true
		workspaceStudio.catalogError = null
		try {
			await withLiveSession(activeId, (sid) =>
				applyEngineTableAdmin({
					sessionId: sid,
					op: 'createObject',
					name,
					schema: group || null,
					database: currentDatabaseName,
					kind,
					profileReadOnly: !!activeSession.profile.readOnly,
				}),
			)
			await refreshCurrentTables()
		} catch (e) {
			workspaceStudio.catalogError = e instanceof Error ? e.message : String(e)
		} finally {
			createBusy = false
		}
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
				workspaceStudio.sideMode = 'explorer'
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
			const updated = await connectionProfiles.touchConnected(profile.id)
			const session = await ensureEngineSession({
				engine: String(profile.engine),
				pluginConfig,
				password,
				pool: { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 },
				sessionId: profile.id,
			})
			workspaceSession.connect(
				{ ...updated, status: 'connected' },
				session.sessionId,
				session.capabilities,
			)
			workspaceStudio.sideMode = 'explorer'
			pickerOpen = false
		} catch (e) {
			pickerError = e instanceof Error ? e.message : String(e)
		} finally {
			pickerBusy = false
		}
	}

	async function refreshConnection(id: string) {
		onSelectConnection(id)
		if (!activeSession) return
		const engine = String(activeSession.profile.engine)
		const preferred = sessionDatabase(id, activeSession.profile)
		if (!isTauri()) {
			await refreshDatabases()
			await refreshCurrentTables()
			return
		}
		workspaceStudio.liveCatalogKey = {
			...workspaceStudio.liveCatalogKey,
			[id]: '',
		}
		const sid = await reopenEngineSession(activeSession.profile, preferred)
		await ensureLiveCatalog(id, engine, sid, preferred, true, true)
	}

	function openNewQueryOn(id: string) {
		onSelectConnection(id)
		workspaceSession.openNewQuery(id)
	}

	function handleDisconnect(id: string) {
		const session = sessions.find((s) => s.profile.id === id)
		const sid = session?.engineSessionId
		workspaceSession.disconnect(id)
		const { [id]: _c, ...restCat } = workspaceStudio.catalogs
		const { [id]: _d, ...restDb } = workspaceStudio.selectedDb
		const { [id]: _k, ...restKey } = workspaceStudio.liveCatalogKey
		const { [id]: _f, ...restFlight } = workspaceStudio.catalogLoadInFlight
		workspaceStudio.catalogs = restCat
		workspaceStudio.selectedDb = restDb
		workspaceStudio.liveCatalogKey = restKey
		workspaceStudio.catalogLoadInFlight = restFlight
		const nextModels: typeof workspaceStudio.liveModels = {}
		for (const [k, v] of Object.entries(workspaceStudio.liveModels)) {
			if (!k.startsWith(`${id}::`)) nextModels[k] = v
		}
		workspaceStudio.liveModels = nextModels
		if (sid) catalogColumnsCache.invalidate(sid)
		if (workspaceStudio.savedQueriesFor === id) {
			workspaceStudio.savedQueries = []
			workspaceStudio.savedQueriesFor = null
		}
		if (workspaceStudio.historyFor === id) {
			workspaceStudio.queryHistory = []
			workspaceStudio.historyFor = null
		}
		if (sid && isTauri()) {
			void closeEngineSession(sid).catch(() => {})
		}
		if (workspaceSession.sessions.length === 0) goto(resolve('/'))
	}

	function setSideMode(mode: SideNavMode) {
		workspaceStudio.sideMode = mode
		if (mode === 'saved-queries') {
			workspaceStudio.selectedHistoryId = null
			void refreshSavedQueries(activeId)
			// Keep editor visible; do not auto-open the first query.
		} else if (mode === 'history') {
			workspaceStudio.selectedQueryId = null
			void refreshHistory({ preferSelectLatest: true })
		}
	}

	function onSelectSavedQuery(id: string) {
		workspaceStudio.selectedQueryId = id
		const q = workspaceStudio.savedQueries.find((s) => s.id === id)
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
		action: 'duplicate' | 'rename' | 'delete' | 'favorite',
		id: string,
	) {
		const q = workspaceStudio.savedQueries.find((s) => s.id === id)
		if (!q || !activeId) return

		if (action === 'favorite') {
			try {
				await saveQuery({
					id: q.id,
					connectionId: q.connectionId || activeId,
					name: q.name,
					body: q.sql,
					language: queryLanguageFor(
						String(activeSession?.profile.engine ?? ''),
						activeSession?.capabilities,
					),
					favorite: !q.favorite,
				})
				await refreshSavedQueries(activeId, true)
			} catch (e) {
				alert(e instanceof Error ? e.message : String(e))
			}
			return
		}

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
					language: queryLanguageFor(
						String(activeSession?.profile.engine ?? ''),
						activeSession?.capabilities,
					),
				})
				await refreshSavedQueries(activeId, true)
				workspaceStudio.selectedQueryId = saved.id
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
		const q = id ? workspaceStudio.savedQueries.find((s) => s.id === id) : null
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
				language: queryLanguageFor(
					String(activeSession?.profile.engine ?? ''),
					activeSession?.capabilities,
				),
			})
			workspaceSession.renameSavedQueryTabs(id, saved.name)
			await refreshSavedQueries(activeId, true)
			workspaceStudio.selectedQueryId = saved.id
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
			if (workspaceStudio.selectedQueryId === id)
				workspaceStudio.selectedQueryId = null
			await refreshSavedQueries(activeId, true)
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
	/** Per-tab preview generation. Stale fetches are ignored, not cancelled. */
	let previewEpochByTab = $state<Record<string, number>>({})

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
		if (workspaceStudio.loadingTabId === tab.id) return
		hydratedTableTabs = { ...hydratedTableTabs, [tab.id]: true }
		void openTable(table)
	})

	function onSelectConnection(id: string) {
		workspaceStudio.sideMode = 'explorer'
		workspaceSession.setActiveConnection(id)
	}

	function onInspectorFieldChange(column: string, value: unknown) {
		if (activeSession?.profile.readOnly) return
		if (rowSelectionIndex === null || !gridTable) return
		const i = rowSelectionIndex
		const current = gridTable.rows[i] ?? {}
		gridTable.rows[i] = { ...current, [column]: value }
		workspaceStudio.rowSyncTick += 1
	}

	function onGridBaselineChange() {
		workspaceStudio.baselineEpoch += 1
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
		workspaceStudio.sideMode = 'explorer'
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
		codeFilter?: string
		sortColumn?: string
		sortDir?: TablePreviewSort['dir']
	}) {
		if (!gridTable) return
		if (activeTabId) {
			const { [activeTabId]: _done, ...rest } = hydratedTableTabs
			hydratedTableTabs = rest
		}
		const sort =
			opts?.sortColumn && opts?.sortDir
				? { column: opts.sortColumn, dir: opts.sortDir }
				: null
		await openTable(
			gridTable,
			opts?.offset ?? 0,
			opts?.filters ?? [],
			sort,
			opts?.codeFilter ?? null,
			true,
		)
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
		mode={workspaceStudio.sideMode}
		onselect={onSelectConnection}
		ondisconnect={handleDisconnect}
		onrefresh={(id) => void refreshConnection(id)}
		onnewquery={openNewQueryOn}
		onmetrics={(id) => workspaceSession.openMetrics(id)}
		onopen={() => {
			pickerError = null
			pickerOpen = true
		}}
		onmode={setSideMode}
		onreorder={(movedId, beforeId) =>
			workspaceSession.reorderSessions(movedId, beforeId)}
	/>

	<WorkspaceSidePane
		sideMode={workspaceStudio.sideMode}
		{activeSession}
		{databases}
		{currentDatabaseName}
		activeTableId={activeTableIdForExplorer}
		refreshing={workspaceStudio.refreshing}
		queryHistory={workspaceStudio.queryHistory}
		savedQueries={workspaceStudio.savedQueries}
		selectedQueryId={workspaceStudio.selectedQueryId}
		selectedHistoryId={workspaceStudio.selectedHistoryId}
		onselectquery={onSelectSavedQuery}
		onselecthistory={onSelectHistory}
		onrefreshsaved={() => void refreshSavedQueries(activeId, true)}
		onrefreshhistory={() => void refreshHistory({ force: true })}
		onselectdatabase={selectDatabase}
		onrefresh={refreshDatabases}
		onrefreshtables={() => void refreshCurrentTables()}
		{refreshingTables}
		oncreatedatabase={createDatabase}
		oncreategroup={(name) => void createCatalogGroup(name)}
		oncreateobject={(name, group) => void createCatalogObject(name, group)}
		{createBusy}
		createError={null}
		onopen={openTable}
		ontableaction={(action, table, extra) =>
			void onTableExplorerAction(action, table, extra)}
		onsavedaction={(action, id) => void onSavedQueryAction(action, id)}
	/>

	<WorkspaceMainPane
		sideMode={workspaceStudio.sideMode}
		{activeId}
		{activeSession}
		{connectionTabs}
		{activeTabId}
		{activeTab}
		{currentDatabaseName}
		catalogError={workspaceStudio.catalogError}
		modelError={workspaceStudio.modelError}
		modelLoading={workspaceStudio.modelLoading}
		{activeModel}
		structure={activeStructure?.data ?? null}
		structureLoading={workspaceStudio.structureLoading}
		structureError={workspaceStudio.structureError}
		structureFetchedAt={activeStructure?.rev ?? null}
		onopentable={structureTableMeta
			? () => void openTable(structureTableMeta)
			: undefined}
		{gridTable}
		tableLoading={workspaceStudio.loadingTabId === activeTabId}
		{rowSelectionIndex}
		rowSyncTick={workspaceStudio.rowSyncTick}
		tablePreviewTick={workspaceStudio.tablePreviewTick}
		baselineEpoch={workspaceStudio.baselineEpoch}
		{inspectorData}
		onhistory={(item) => prependHistoryItem(item)}
		onbaselinechange={onGridBaselineChange}
		onselectrow={onSelectGridRow}
		onopenmodelretry={() => {
			if (modelTableMeta) void openModelDiagram(true, { table: modelTableMeta })
		}}
		onopenstructureretry={() => {
			if (structureTableMeta) void openObjectStructure(structureTableMeta, true)
		}}
		oninspectorchange={onInspectorFieldChange}
		onsavequery={() => void saveActiveQuery()}
		querysaving={workspaceStudio.querySaving}
		onclosetab={requestCloseTab}
		onclosemany={enqueueCloseTabs}
		onreloadtable={reloadGridTable}
		onreconnect={reconnectActiveSession}
		onopenreference={(ref, value) => void openForeignKeyReference(ref, value)}
		onopenstructure={(table) => void openObjectStructure(table)}
		{objectKindById}
	/>

	<ImportDataModal
		open={!!workspaceStudio.importTable && !workspaceStudio.importPreview}
		objectLabel={workspaceStudio.importTable?.kind === 'collection'
			? 'collection'
			: 'table'}
		formats={activeSession
			? resolveCapabilities(
					String(activeSession.profile.engine),
					activeSession.capabilities,
				).exportFormats
			: ['csv', 'json', 'jsonl']}
		busy={workspaceStudio.importBusy}
		error={workspaceStudio.importError}
		onconfirm={(spec) => void confirmImportFile(spec)}
		oncancel={closeImport}
	/>

	<ImportPreviewModal
		open={!!workspaceStudio.importTable && !!workspaceStudio.importPreview}
		objectName={workspaceStudio.importTable?.name ?? ''}
		preview={workspaceStudio.importPreview}
		busy={workspaceStudio.importBusy}
		error={workspaceStudio.importError}
		onencoding={(enc) => {
			if (!workspaceStudio.importTable || !workspaceStudio.importPath) return
			void loadImportPreview(
				workspaceStudio.importTable,
				workspaceStudio.importPath,
				workspaceStudio.importFormat,
				enc,
			)
		}}
		onconfirm={(spec) => void confirmImportRun(spec)}
		onback={() => {
			if (workspaceStudio.importBusy) return
			workspaceStudio.importPreview = null
			workspaceStudio.importError = null
		}}
		oncancel={closeImport}
	/>

	<ExportDataModal
		open={!!workspaceStudio.exportTable}
		objectName={workspaceStudio.exportTable?.name ?? ''}
		objectLabel={workspaceStudio.exportTable?.kind === 'collection'
			? 'collection'
			: workspaceStudio.exportTable?.kind === 'view'
				? 'view'
				: 'table'}
		formats={activeSession
			? resolveCapabilities(
					String(activeSession.profile.engine),
					activeSession.capabilities,
				).exportFormats
			: ['csv', 'json', 'jsonl']}
		busy={workspaceStudio.exportBusy}
		error={workspaceStudio.exportError}
		onconfirm={(spec) => void confirmExport(spec)}
		oncancel={() => {
			if (workspaceStudio.exportBusy) return
			workspaceStudio.exportTable = null
			workspaceStudio.exportError = null
		}}
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
		busy={closePromptBusy || workspaceStudio.querySaving}
		onsave={() => void confirmSaveAndClose()}
		ondiscard={() => void confirmDiscardAndClose()}
		oncancel={cancelCloseTab}
	/>

	<SaveQueryModal
		open={saveNameModalOpen}
		initialName={saveNameInitial}
		busy={workspaceStudio.querySaving}
		error={saveNameError}
		onconfirm={(name) => void onSaveNameConfirm(name)}
		oncancel={() => {
			if (workspaceStudio.querySaving) return
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

	<Modal
		open={deleteModalOpen}
		onclose={() => {
			if (deleteBusy) return
			deleteModalOpen = false
			deleteQueryId = null
		}}
		title="Delete query"
		description="Delete “{deleteQueryName}”? This cannot be undone. Open tabs linked to this query will be closed."
		dismissible={!deleteBusy}
	>
		{#snippet footer()}
			<Button
				variant="ghost"
				size="sm"
				disabled={deleteBusy}
				onclick={() => {
					deleteModalOpen = false
					deleteQueryId = null
				}}
			>
				Cancel
			</Button>
			<Button
				variant="danger"
				size="sm"
				disabled={deleteBusy}
				onclick={() => void confirmDeleteSavedQuery()}
			>
				{deleteBusy ? 'Deleting…' : 'Delete'}
			</Button>
		{/snippet}
	</Modal>
</div>
