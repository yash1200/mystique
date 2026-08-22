<script lang="ts">
	import { goto } from '$app/navigation'
	import { resolve } from '$app/paths'
	import {
		loadLiveCatalog,
		loadTablePreview,
		mergeCatalogKeepCached,
		mergeCatalogReplaceDatabase,
		preferDatabase,
	} from '$lib/api/catalog'
	import {
		applyEngineTableAdmin,
		closeEngineSession,
		describeEngineObjectStructure,
		ensureEngineSession,
		exportEngineObject,
		importEngineObject,
		openEngineSession,
		previewEngineImport,
		testEngineConnection,
		type ImportEncoding,
		type ImportFormat,
		type ObjectStructure,
		type PreviewImportResult,
		type TableAdminOp,
		type TablePreviewFilter,
		type TablePreviewSort,
	} from '$lib/api/engines'
	import {
		historyItemToEntry,
		listQueryHistory,
		listRecentQueryHistory,
		type QueryHistoryEntry,
		type QueryHistoryItem,
	} from '$lib/api/history'
	import { loadLiveErDiagram } from '$lib/api/liveEr'
	import { touchProfileConnected } from '$lib/api/profiles'
	import {
		deleteSavedQuery,
		listSavedQueries,
		saveQuery,
		toListSavedQuery,
		type SavedQueryListItem,
	} from '$lib/api/queries'
	import { isTauri } from '$lib/api/tauri'
	import { Button, Modal } from '$lib/components/core'
	import { workspaceSession } from '$lib/stores/workspace-session.svelte'
	import {
		colorHex,
		draftFromProfile,
		pluginConfigForSession,
		type ConnectionProfile,
	} from '$lib/types/connection'
	import { modelTabTitle, resolveCapabilities } from '$lib/types/engineCaps'
	import type { ErDiagramState } from '$lib/types/er'
	import type {
		CatalogObjectKind,
		ColumnReference,
		DatabaseMeta,
		TableMeta,
	} from '$lib/types/workspace'
	import { setAppTitle } from '$lib/utils/appTitle'
	import { queryLanguageFor } from '$lib/utils/queryPolicyFe'
	import { onDestroy, onMount } from 'svelte'
	import ExportDataModal, {
		type ExportDataSpec,
	} from './ExportDataModal.svelte'
	import ImportDataModal, {
		type ImportFileSpec,
	} from './ImportDataModal.svelte'
	import ImportPreviewModal, {
		type ImportRunSpec,
	} from './ImportPreviewModal.svelte'
	import type { TableExplorerAction } from './ObjectExplorer.svelte'
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

	// OS window title: "Mystique DB · {connection}" (system title bar only).
	$effect(() => {
		void setAppTitle(activeSession?.profile.name ?? null)
	})
	onDestroy(() => {
		void setAppTitle()
	})

	let catalogs = $state<Record<string, DatabaseMeta[]>>({})
	let selectedDb = $state<Record<string, string>>({})
	let refreshing = $state(false)
	let tableLoading = $state(false)
	let catalogError = $state<string | null>(null)
	let exportTable = $state<TableMeta | null>(null)
	let exportBusy = $state(false)
	let exportError = $state<string | null>(null)
	let importTable = $state<TableMeta | null>(null)
	let importBusy = $state(false)
	let importError = $state<string | null>(null)
	let importPath = $state('')
	let importFormat = $state<ImportFormat>('csv')
	let importPreview = $state<PreviewImportResult | null>(null)
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
	let savedQueries = $state<SavedQueryListItem[]>([])
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

	/** Object structure keyed by connectionId::tableId */
	let liveStructures = $state<
		Record<string, { data: ObjectStructure; rev: number }>
	>({})
	let structureLoading = $state(false)
	let structureError = $state<string | null>(null)

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
		const selected = selectedDb[connectionId]?.trim()
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
		const existing = catalogs[connectionId]
		if (
			!force &&
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
				includeObjects,
			})
			catalogs = {
				...catalogs,
				[connectionId]: mergeCatalogKeepCached(catalogs[connectionId], live),
			}
			liveCatalogKey = { ...liveCatalogKey, [connectionId]: sessionId }
			const names = live.map((d) => d.name)
			const pick = preferDatabase(
				names,
				preferredDatabase,
				systemDbNames(engine, connectionId),
			)
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
						includeObjects,
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
					const pick = preferDatabase(
						names,
						preferredDatabase,
						systemDbNames(engine, connectionId),
					)
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
			if (!catalogs[connectionId]) {
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
					catalogError = e instanceof Error ? e.message : String(e)
					return
				}
			}
			await ensureLiveCatalog(id, engine, sid, preferred)
		})()
	})

	const databases = $derived(activeId ? (catalogs[activeId] ?? []) : [])

	const objectKindById = $derived.by(() => {
		const map: Record<string, CatalogObjectKind> = {}
		for (const db of databases) {
			for (const t of db.tables) map[t.id] = t.kind
		}
		return map
	})

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
			liveStructures[structureCacheKey(activeId, activeTab.tableId)] ?? null
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
		const cat = catalogs[activeId] ?? []
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
			structureLoading ||
			structureError
		) {
			return
		}
		const key = structureCacheKey(activeId, activeTab.tableId)
		if (liveStructures[key]?.data) return
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
				finishQueuedClose(tabId)
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
				tab.title && !needsNamePrompt(tab.title) ? tab.title : 'Query'
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
	function openSqlInEditor(sql: string, title?: string) {
		if (!activeId) return
		workspaceSession.openNewQueryWithText(activeId, sql, title)
	}

	function structureCacheKey(connectionId: string, tableId: string): string {
		return `${connectionId}::${tableId}`
	}

	async function openObjectStructure(table: TableMeta, force = false) {
		if (!activeId || !activeSession) return
		sideMode = 'explorer'
		const title = `Structure · ${table.name}`
		workspaceSession.openStructure(activeId, table.id, title)
		const key = structureCacheKey(activeId, table.id)
		if (!force && liveStructures[key]?.data?.sections?.length) {
			structureError = null
			return
		}
		if (!isTauri()) {
			structureError =
				'Object structure requires Tauri. Run `npm run tauri dev` and Connect.'
			return
		}
		structureLoading = true
		structureError = null
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
			liveStructures = {
				...liveStructures,
				[key]: { data: loaded, rev: Date.now() },
			}
			if (loaded.title) {
				workspaceSession.openStructure(
					activeId,
					table.id,
					`Structure · ${table.name}`,
				)
			}
		} catch (e) {
			structureError = e instanceof Error ? e.message : String(e)
		} finally {
			structureLoading = false
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
			void openModelDiagram(false)
			return
		}
		if (action === 'structure') {
			void openObjectStructure(table)
			return
		}
		if (action === 'export') {
			exportTable = table
			exportError = null
			return
		}
		if (action === 'import') {
			importTable = table
			importError = null
			importPreview = null
			importPath = ''
			return
		}
		if (!isTauri()) {
			catalogError = 'Live catalog actions require the desktop app.'
			return
		}

		const newName = extra?.trim() || null
		if ((action === 'rename' || action === 'duplicate') && !newName) return

		const op: TableAdminOp = action === 'create' ? 'createStatement' : action

		try {
			catalogError = null
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
					catalogError = 'Engine returned an empty create statement.'
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
				if (liveStructures[sk]) {
					const { [sk]: _drop, ...rest } = liveStructures
					liveStructures = rest
				}
			} else if (action === 'truncate' && tableOpen) {
				void openTable(table)
			}
			await refreshCurrentTables()
		} catch (e) {
			catalogError = e instanceof Error ? e.message : String(e)
		}
	}

	function closeImport() {
		if (importBusy) return
		importTable = null
		importPreview = null
		importError = null
		importPath = ''
	}

	async function loadImportPreview(
		table: TableMeta,
		path: string,
		format: ImportFormat,
		encoding: ImportEncoding,
	) {
		if (!activeId) return
		importBusy = true
		importError = null
		try {
			importPreview = await withLiveSession(activeId, (sid) =>
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
			importPath = path
			importFormat = format
		} catch (e) {
			importError = e instanceof Error ? e.message : String(e)
		} finally {
			importBusy = false
		}
	}

	async function confirmImportFile(spec: ImportFileSpec) {
		if (!importTable) return
		if (!isTauri()) {
			importError = 'Import requires the desktop app.'
			return
		}
		await loadImportPreview(importTable, spec.path, spec.format, 'utf8')
	}

	async function confirmImportRun(spec: ImportRunSpec) {
		if (!importTable || !activeId || !importPath) return
		const table = importTable
		importBusy = true
		importError = null
		try {
			await withLiveSession(activeId, (sid) =>
				importEngineObject({
					sessionId: sid,
					name: table.name,
					schema: table.schema || null,
					database: table.database || currentDatabaseName,
					kind: table.kind,
					path: importPath,
					format: importFormat,
					encoding: spec.encoding,
					mapping: spec.mapping,
					onDuplicate: spec.onDuplicate,
					profileReadOnly: !!activeSession?.profile.readOnly,
				}),
			)
			importTable = null
			importPreview = null
			importPath = ''
			const tabId = `${activeId}::table::${table.id}`
			if (workspaceSession.openTabs.some((t) => t.id === tabId)) {
				void openTable(table)
			}
		} catch (e) {
			importError = e instanceof Error ? e.message : String(e)
		} finally {
			importBusy = false
		}
	}

	async function confirmExport(spec: ExportDataSpec) {
		if (!exportTable || !activeId) return
		if (!isTauri()) {
			exportError = 'Export requires the desktop app.'
			return
		}
		const table = exportTable
		exportBusy = true
		exportError = null
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
			exportTable = null
		} catch (e) {
			exportError = e instanceof Error ? e.message : String(e)
		} finally {
			exportBusy = false
		}
	}

	function onSelectHistory(id: string) {
		selectedHistoryId = id
		const h = queryHistory.find((x) => x.id === id)
		if (!h || !activeId) return
		// Open the run directly in the query editor (no preview pane).
		openSqlInEditor(h.sql)
	}

	/** Filters applied when opening a table via FK jump (seed DataGrid filter bar). */
	let tableSeedFilters = $state<TablePreviewFilter[] | null>(null)
	/** Raw code-filter box so remounts stay in code mode. */
	let tableSeedCodeFilter = $state<string | null>(null)
	/** Preview sort so remounts (filter apply, FK jump) keep the same column. */
	let tableSeedSort = $state<TablePreviewSort | null>(null)

	async function openTable(
		table: TableMeta,
		offset = 0,
		filters: TablePreviewFilter[] = [],
		sort: TablePreviewSort | null = null,
		codeFilter: string | null = null,
	) {
		if (!activeId || !activeSession) return
		sideMode = 'explorer'
		tableSeedFilters = filters.length > 0 ? filters : null
		tableSeedCodeFilter = codeFilter?.trim() || null
		tableSeedSort = sort
		workspaceSession.openTable(activeId, table.id, table.name)

		if (!isTauri()) return

		// Always fetch live preview so reseed / external writes show up.
		tableLoading = true
		catalogError = null
		try {
			const engine = String(activeSession.profile.engine)
			const preview = await withLiveSession(activeId, (sid) =>
				loadTablePreview(
					sid,
					engine,
					table,
					100,
					offset,
					filters,
					sort,
					codeFilter,
				),
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

	/** Resolve a plugin FK target to a catalog TableMeta (same connection). */
	function findReferencedTable(ref: ColumnReference): TableMeta | null {
		if (!activeId) return null
		const list = catalogs[activeId] ?? []
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
			catalogError = `Referenced table “${ref.schema ? `${ref.schema}.` : ''}${ref.table}” is not in the explorer catalog.`
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
					/* selectDatabase sets catalogError */
				}
			} else {
				selectedDb = { ...selectedDb, [activeId!]: targetDb }
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
		if (!next || selectedDb[activeId] === next) return
		const previous = selectedDb[activeId] ?? ''
		selectedDb = { ...selectedDb, [activeId]: next }

		const engine = String(activeSession.profile.engine)
		if (
			!isTauri() ||
			!engineBindsSessionToDatabase(engine, activeSession?.capabilities)
		)
			return

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
	async function openModelDiagram(
		force = false,
		databaseOverride?: string | null,
	) {
		if (!activeSession || !activeId) return
		sideMode = 'explorer'
		const engine = String(activeSession.profile.engine)
		const database = (databaseOverride ?? currentDatabaseName ?? '').trim()
		const title = modelTabTitle(
			resolveCapabilities(engine, activeSession.capabilities),
			database,
		)

		const key = modelCacheKey(activeId, database)
		const alreadyOpen =
			activeTab?.kind === 'model' &&
			(activeTab.modelDatabase ?? '') === database
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
			const loaded = await loadLiveErDiagram(
				sid,
				engine,
				database || null,
				activeSession.capabilities,
			)
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
					const loaded = await loadLiveErDiagram(
						sid,
						engine,
						database || null,
						activeSession.capabilities,
					)
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

	// Rebuild when a model tab is focused without a cached diagram
	// (reconnect / app reopen restores the tab but liveModels is empty).
	$effect(() => {
		if (
			!activeId ||
			!activeSession ||
			!activeTab ||
			activeTab.kind !== 'model' ||
			modelLoading ||
			modelError
		) {
			return
		}
		const db = activeTab.modelDatabase ?? currentDatabaseName ?? ''
		if (liveModels[modelCacheKey(activeId, db)]?.diagram) return
		void openModelDiagram(true, db)
	})

	async function refreshDatabases() {
		if (!activeSession || !activeId) return
		const engine = String(activeSession.profile.engine)
		const preferred = sessionDatabase(activeId, activeSession.profile)

		if (!isTauri()) return

		refreshing = true
		catalogError = null
		try {
			const sid = await withLiveSession(
				activeId,
				async (sessionId) => sessionId,
			)
			await ensureLiveCatalog(activeId, engine, sid, preferred, true, false)
		} catch (e) {
			catalogError = e instanceof Error ? e.message : String(e)
			refreshing = false
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
		catalogError = null
		try {
			const sid = await withLiveSession(
				activeId,
				async (sessionId) => sessionId,
			)
			const live = await loadLiveCatalog(sid, engine, {
				preferredDatabase: db,
				includeObjects: true,
			})
			catalogs = {
				...catalogs,
				[activeId]: mergeCatalogReplaceDatabase(
					catalogs[activeId],
					live,
					db || '',
				),
			}
		} catch (e) {
			catalogError = e instanceof Error ? e.message : String(e)
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
		const list = [...(catalogs[activeId] ?? []), entry]
		catalogs = { ...catalogs, [activeId]: list }
		selectedDb = { ...selectedDb, [activeId]: name }
	}

	let createBusy = $state(false)

	async function createCatalogGroup(name: string) {
		if (!activeId || !activeSession) return
		// Mock catalog: group is empty until an object is added; explorer keeps localGroups.
		if (!isTauri()) {
			catalogError = null
			return
		}
		createBusy = true
		catalogError = null
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
			catalogError = e instanceof Error ? e.message : String(e)
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
			const list = (catalogs[activeId] ?? []).map((d) => {
				if (d.name !== dbName) return d
				if (d.tables.some((t) => t.id === id)) return d
				return { ...d, tables: [...d.tables, table] }
			})
			catalogs = { ...catalogs, [activeId]: list }
			catalogError = null
			return
		}
		createBusy = true
		catalogError = null
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
			catalogError = e instanceof Error ? e.message : String(e)
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
		if (!activeSession) return
		const engine = String(activeSession.profile.engine)
		const preferred = sessionDatabase(id, activeSession.profile)
		if (!isTauri()) {
			await refreshDatabases()
			await refreshCurrentTables()
			return
		}
		liveCatalogKey = { ...liveCatalogKey, [id]: '' }
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
					language: queryLanguageFor(
						String(activeSession?.profile.engine ?? ''),
						activeSession?.capabilities,
					),
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
				language: queryLanguageFor(
					String(activeSession?.profile.engine ?? ''),
					activeSession?.capabilities,
				),
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
		onrefreshtables={() => void refreshCurrentTables()}
		{refreshingTables}
		oncreatedatabase={createDatabase}
		oncreategroup={(name) => void createCatalogGroup(name)}
		oncreateobject={(name, group) => void createCatalogObject(name, group)}
		{createBusy}
		createError={null}
		onopen={openTable}
		onopenmodel={() => void openModelDiagram(false)}
		ontableaction={(action, table, extra) =>
			void onTableExplorerAction(action, table, extra)}
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
		structure={activeStructure?.data ?? null}
		{structureLoading}
		{structureError}
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
		onopenstructureretry={() => {
			if (structureTableMeta) void openObjectStructure(structureTableMeta, true)
		}}
		oninspectorchange={onInspectorFieldChange}
		onsavequery={() => void saveActiveQuery()}
		querysaving={querySaving}
		onclosetab={requestCloseTab}
		onclosemany={enqueueCloseTabs}
		onreloadtable={reloadGridTable}
		onreconnect={reconnectActiveSession}
		seedFilters={tableSeedFilters}
		seedCodeFilter={tableSeedCodeFilter}
		seedSort={tableSeedSort}
		onopenreference={(ref, value) => void openForeignKeyReference(ref, value)}
		{objectKindById}
	/>

	<ImportDataModal
		open={!!importTable && !importPreview}
		objectLabel={importTable?.kind === 'collection' ? 'collection' : 'table'}
		formats={activeSession
			? resolveCapabilities(
					String(activeSession.profile.engine),
					activeSession.capabilities,
				).exportFormats
			: ['csv', 'json', 'jsonl']}
		busy={importBusy}
		error={importError}
		onconfirm={(spec) => void confirmImportFile(spec)}
		oncancel={closeImport}
	/>

	<ImportPreviewModal
		open={!!importTable && !!importPreview}
		objectName={importTable?.name ?? ''}
		preview={importPreview}
		busy={importBusy}
		error={importError}
		onencoding={(enc) => {
			if (!importTable || !importPath) return
			void loadImportPreview(importTable, importPath, importFormat, enc)
		}}
		onconfirm={(spec) => void confirmImportRun(spec)}
		onback={() => {
			if (importBusy) return
			importPreview = null
			importError = null
		}}
		oncancel={closeImport}
	/>

	<ExportDataModal
		open={!!exportTable}
		objectName={exportTable?.name ?? ''}
		objectLabel={exportTable?.kind === 'collection'
			? 'collection'
			: exportTable?.kind === 'view'
				? 'view'
				: 'table'}
		formats={activeSession
			? resolveCapabilities(
					String(activeSession.profile.engine),
					activeSession.capabilities,
				).exportFormats
			: ['csv', 'json', 'jsonl']}
		busy={exportBusy}
		error={exportError}
		onconfirm={(spec) => void confirmExport(spec)}
		oncancel={() => {
			if (exportBusy) return
			exportTable = null
			exportError = null
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
