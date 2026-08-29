/**
 * Connected-session state for the workspace shell.
 * Open tabs + query drafts persist to the app SQLite DB (via Tauri IPC).
 */
import type {
	ConnectedSession,
	ConnectionProfile,
	EngineCapabilities,
} from '$core/contracts'
import { isTauri } from '$core/ipc'
import { normalizeCapabilities } from '$engines'
import type { QueryRunCache } from '$features/workspace/query'
import {
	listWorkspaceTabs,
	recordToWorkspaceTab,
	replaceWorkspaceTabs,
	workspaceTabToSaveInput,
} from './api'
import type { SelectedRow, WorkspaceTab } from './types'

export type { ConnectedSession } from '$core/contracts'

class WorkspaceSessionStore {
	sessions = $state<ConnectedSession[]>([])
	activeConnectionId = $state<string | null>(null)
	openTabs = $state<WorkspaceTab[]>([])
	activeTabId = $state<string | null>(null)
	/** Last focused tab for each connection. `activeTabId` is only the current one. */
	private activeTabByConnection = $state<Record<string, string>>({})
	selectedRow = $state<SelectedRow | null>(null)
	/** Prompt waiting for the query agent pane to send. */
	pendingAgentPrompt = $state<string | null>(null)

	/** Draft SQL/MQL text per query tab id. */
	queryDrafts = $state<Record<string, string>>({})
	/** Baseline body used to compute dirty (saved body or last restore). */
	queryBaselines = $state<Record<string, string>>({})
	/**
	 * Last finished query run per tab. Plain Map on purpose: not $state.
	 * Result sets are large; a reactive store + $effect copy loop froze the app.
	 */
	private queryRunCache = new Map<string, QueryRunCache>()

	/** Debounced persist handles per connection. */
	private persistTimers = new Map<string, ReturnType<typeof setTimeout>>()
	private restoring = new Set<string>()

	get activeSession(): ConnectedSession | null {
		return (
			this.sessions.find((s) => s.profile.id === this.activeConnectionId) ??
			null
		)
	}

	queryTabId(connectionId: string): string {
		const existing = this.openTabs.find(
			(t) => t.connectionId === connectionId && t.kind === 'query',
		)
		return existing?.id ?? `${connectionId}::query`
	}

	get activeTab(): WorkspaceTab | null {
		return this.openTabs.find((t) => t.id === this.activeTabId) ?? null
	}

	private rememberTab(connectionId: string, tabId: string | null) {
		if (tabId) {
			if (this.activeTabByConnection[connectionId] === tabId) return
			this.activeTabByConnection = {
				...this.activeTabByConnection,
				[connectionId]: tabId,
			}
			return
		}
		if (!(connectionId in this.activeTabByConnection)) return
		const next = { ...this.activeTabByConnection }
		delete next[connectionId]
		this.activeTabByConnection = next
	}

	private focusConnectionTab(connectionId: string, tabId: string) {
		this.activeConnectionId = connectionId
		this.activeTabId = tabId
		this.rememberTab(connectionId, tabId)
	}

	private tabOnConnection(
		connectionId: string,
		tabId: string | null | undefined,
	): WorkspaceTab | undefined {
		if (!tabId) return undefined
		return this.openTabs.find(
			(t) => t.connectionId === connectionId && t.id === tabId,
		)
	}

	/** Register / refresh a live session and focus it. Restores tabs from SQLite. */
	connect(
		profile: ConnectionProfile,
		engineSessionId?: string | null,
		capabilities?: EngineCapabilities | null,
	) {
		const now = new Date().toISOString()
		const existing = this.sessions.findIndex((s) => s.profile.id === profile.id)
		const prev = existing >= 0 ? this.sessions[existing] : null
		const next: ConnectedSession = {
			profile: { ...profile, status: 'connected' },
			connectedAt: now,
			engineSessionId:
				engineSessionId !== undefined
					? engineSessionId
					: (prev?.engineSessionId ?? null),
			capabilities:
				capabilities !== undefined
					? capabilities
						? normalizeCapabilities(capabilities, profile.engine)
						: null
					: (prev?.capabilities ?? null),
		}
		if (existing >= 0) {
			this.sessions = this.sessions.map((s, i) => (i === existing ? next : s))
		} else {
			this.sessions = [...this.sessions, next]
		}
		this.setActiveConnection(profile.id)
		void this.restoreTabsForConnection(profile.id)
	}

	/**
	 * Load persisted tabs for a connection (on connect / app reopen).
	 * Skips if tabs for this connection already exist in memory.
	 */
	async restoreTabsForConnection(connectionId: string) {
		if (!isTauri()) return
		if (this.restoring.has(connectionId)) return
		const hasMemory = this.openTabs.some((t) => t.connectionId === connectionId)
		if (hasMemory) return

		this.restoring.add(connectionId)
		try {
			const records = await listWorkspaceTabs(connectionId)
			if (records.length === 0) return

			const tabs = records.map(recordToWorkspaceTab)
			const drafts: Record<string, string> = { ...this.queryDrafts }
			const baselines: Record<string, string> = { ...this.queryBaselines }

			for (const r of records) {
				if (r.kind === 'query') {
					const body = r.body ?? ''
					drafts[r.id] = body
					baselines[r.id] = body
				}
			}

			this.openTabs = [...this.openTabs, ...tabs]
			this.queryDrafts = drafts
			this.queryBaselines = baselines

			const active =
				records.find((r) => r.isActive)?.id ??
				records[records.length - 1]?.id ??
				null
			if (active) {
				this.rememberTab(connectionId, active)
				if (this.activeConnectionId === connectionId) {
					this.activeTabId = active
				}
			}
		} catch (e) {
			console.warn('[mystique] restore workspace tabs:', e)
		} finally {
			this.restoring.delete(connectionId)
		}
	}

	/** Persist open tabs for one connection (debounced). */
	schedulePersist(connectionId: string) {
		if (!isTauri() || this.restoring.has(connectionId)) return
		const prev = this.persistTimers.get(connectionId)
		if (prev) clearTimeout(prev)
		this.persistTimers.set(
			connectionId,
			setTimeout(() => {
				this.persistTimers.delete(connectionId)
				void this.persistNow(connectionId)
			}, 350),
		)
	}

	async persistNow(connectionId: string) {
		if (!isTauri() || this.restoring.has(connectionId)) return
		const tabs = this.openTabs.filter((t) => t.connectionId === connectionId)
		const remembered = this.activeTabByConnection[connectionId]
		const activeTabId = tabs.some((t) => t.id === remembered)
			? remembered
			: this.activeTabId && tabs.some((t) => t.id === this.activeTabId)
				? this.activeTabId
				: (tabs[0]?.id ?? null)
		try {
			await replaceWorkspaceTabs({
				connectionId,
				activeTabId,
				tabs: tabs.map((tab, i) =>
					workspaceTabToSaveInput(tab, {
						sortOrder: i,
						isActive: tab.id === activeTabId,
						body: this.queryDrafts[tab.id] ?? '',
					}),
				),
			})
		} catch (e) {
			console.warn('[mystique] persist workspace tabs:', e)
		}
	}

	/**
	 * Ensure a Query canvas tab exists for this connection and focus one.
	 * Does not create a second query tab if one already exists.
	 */
	openQuery(connectionId: string) {
		const existing = this.openTabs.find(
			(t) => t.connectionId === connectionId && t.kind === 'query',
		)
		if (existing) {
			this.focusConnectionTab(connectionId, existing.id)
			this.selectedRow = null
			this.schedulePersist(connectionId)
			return
		}
		this.openNewQuery(connectionId)
	}

	/** Always open a fresh Query canvas tab (⌘/Ctrl+T, “+ Query”). */
	openNewQuery(connectionId: string) {
		const count = this.openTabs.filter(
			(t) => t.connectionId === connectionId && t.kind === 'query',
		).length
		const id = `${connectionId}::query::${crypto.randomUUID()}`
		const title = count === 0 ? 'Query' : `Query ${count + 1}`
		this.openTabs = [
			...this.openTabs,
			{
				id,
				connectionId,
				kind: 'query',
				title,
				savedQueryId: null,
				dirty: false,
			},
		]
		this.queryDrafts = { ...this.queryDrafts, [id]: '' }
		this.queryBaselines = { ...this.queryBaselines, [id]: '' }
		this.focusConnectionTab(connectionId, id)
		this.selectedRow = null
		this.schedulePersist(connectionId)
	}

	/** New unsaved query tab seeded with SQL/MQL (explorer “View Create”, etc.). */
	openNewQueryWithText(connectionId: string, text: string, title?: string) {
		this.openNewQuery(connectionId)
		const tabId = this.activeTabId
		if (!tabId) return
		if (title?.trim()) {
			const nextTitle = title.trim()
			this.openTabs = this.openTabs.map((t) =>
				t.id === tabId ? { ...t, title: nextTitle } : t,
			)
		}
		this.setQueryDraft(tabId, text)
	}

	/** Open a saved query directly in the editor (or focus + refresh if already open). */
	openSavedQuery(
		connectionId: string,
		saved: { id: string; name: string; body: string },
	) {
		const existing = this.openTabs.find(
			(t) =>
				t.connectionId === connectionId &&
				t.kind === 'query' &&
				t.savedQueryId === saved.id,
		)
		if (existing) {
			// If the tab is clean, reload library body; keep dirty edits intact.
			if (!existing.dirty) {
				this.queryDrafts = { ...this.queryDrafts, [existing.id]: saved.body }
				this.queryBaselines = {
					...this.queryBaselines,
					[existing.id]: saved.body,
				}
				this.openTabs = this.openTabs.map((t) =>
					t.id === existing.id ? { ...t, title: saved.name, dirty: false } : t,
				)
			}
			this.focusConnectionTab(connectionId, existing.id)
			this.selectedRow = null
			this.schedulePersist(connectionId)
			return
		}
		const id = `${connectionId}::query::saved::${saved.id}`
		this.openTabs = [
			...this.openTabs,
			{
				id,
				connectionId,
				kind: 'query',
				title: saved.name,
				savedQueryId: saved.id,
				dirty: false,
			},
		]
		this.queryDrafts = { ...this.queryDrafts, [id]: saved.body }
		this.queryBaselines = { ...this.queryBaselines, [id]: saved.body }
		this.focusConnectionTab(connectionId, id)
		this.selectedRow = null
		this.schedulePersist(connectionId)
	}

	/** Close the active workspace tab (⌘/Ctrl+W). */
	closeActiveTab() {
		if (!this.activeTabId) return
		this.closeTab(this.activeTabId)
	}

	setActiveConnection(id: string) {
		if (!this.sessions.some((s) => s.profile.id === id)) return
		if (this.selectedRow && this.selectedRow.connectionId !== id) {
			this.selectedRow = null
		}
		const tab =
			this.tabOnConnection(id, this.activeTabByConnection[id]) ??
			this.tabOnConnection(id, this.activeTabId) ??
			this.openTabs.find((t) => t.connectionId === id)
		this.activeConnectionId = id
		this.activeTabId = tab?.id ?? null
		if (tab) this.rememberTab(id, tab.id)
	}

	disconnect(id: string) {
		// Persist final tab state before dropping memory.
		void this.persistNow(id)

		this.sessions = this.sessions.filter((s) => s.profile.id !== id)
		const removedTabIds = this.openTabs
			.filter((t) => t.connectionId === id)
			.map((t) => t.id)
		this.openTabs = this.openTabs.filter((t) => t.connectionId !== id)
		const drafts = { ...this.queryDrafts }
		const baselines = { ...this.queryBaselines }
		for (const tid of removedTabIds) {
			delete drafts[tid]
			delete baselines[tid]
			this.queryRunCache.delete(tid)
		}
		this.queryDrafts = drafts
		this.queryBaselines = baselines

		this.rememberTab(id, null)
		if (this.activeConnectionId === id) {
			const nextId = this.sessions[0]?.profile.id ?? null
			this.activeConnectionId = nextId
			if (nextId) {
				const tab =
					this.tabOnConnection(nextId, this.activeTabByConnection[nextId]) ??
					this.openTabs.find((t) => t.connectionId === nextId)
				this.activeTabId = tab?.id ?? null
				if (tab) this.rememberTab(nextId, tab.id)
			} else {
				this.activeTabId = null
			}
		}
		if (this.selectedRow?.connectionId === id) {
			this.selectedRow = null
		}
	}

	/** Drop every live session from memory (engine pools closed by the caller). */
	disconnectAll() {
		const ids = this.sessions.map((s) => s.profile.id)
		for (const id of ids) this.disconnect(id)
	}

	openTable(connectionId: string, tableId: string, title: string) {
		const id = `${connectionId}::table::${tableId}`
		if (!this.openTabs.some((t) => t.id === id)) {
			this.openTabs = [
				...this.openTabs,
				{
					id,
					connectionId,
					kind: 'table',
					tableId,
					title,
					dirty: false,
				},
			]
		}
		this.focusConnectionTab(connectionId, id)
		this.selectedRow = null
		this.schedulePersist(connectionId)
	}

	/** Open / focus object structure tab (columns, indexes, …). */
	openStructure(connectionId: string, tableId: string, title: string) {
		const id = `${connectionId}::structure::${tableId}`
		if (!this.openTabs.some((t) => t.id === id)) {
			this.openTabs = [
				...this.openTabs,
				{
					id,
					connectionId,
					kind: 'structure',
					tableId,
					title,
					dirty: false,
				},
			]
		} else {
			this.openTabs = this.openTabs.map((t) =>
				t.id === id ? { ...t, title } : t,
			)
		}
		this.focusConnectionTab(connectionId, id)
		this.selectedRow = null
		this.schedulePersist(connectionId)
	}

	/** Open / focus the server metrics tab for this connection. */
	openMetrics(connectionId: string, title = 'Server metrics') {
		const id = `${connectionId}::metrics`
		if (!this.openTabs.some((t) => t.id === id)) {
			this.openTabs = [
				...this.openTabs,
				{
					id,
					connectionId,
					kind: 'metrics',
					title,
					dirty: false,
				},
			]
		} else {
			this.openTabs = this.openTabs.map((t) =>
				t.id === id ? { ...t, title } : t,
			)
		}
		this.focusConnectionTab(connectionId, id)
		this.selectedRow = null
		this.schedulePersist(connectionId)
	}

	/** Open / focus a table-scoped live ER tab. */
	openModel(
		connectionId: string,
		database: string,
		title = 'ER diagram',
		tableId?: string,
	) {
		const dbKey = database?.trim() || '_'
		const objectKey = tableId?.trim() || '_'
		const id = `${connectionId}::model::${dbKey}::${objectKey}`
		if (!this.openTabs.some((t) => t.id === id)) {
			this.openTabs = [
				...this.openTabs,
				{
					id,
					connectionId,
					kind: 'model',
					tableId: tableId?.trim() || undefined,
					modelDatabase: database?.trim() || '',
					title,
					dirty: false,
				},
			]
		} else {
			this.openTabs = this.openTabs.map((t) =>
				t.id === id ? { ...t, title } : t,
			)
		}
		this.focusConnectionTab(connectionId, id)
		this.selectedRow = null
		this.schedulePersist(connectionId)
	}

	closeTab(tabId: string) {
		const tab = this.openTabs.find((t) => t.id === tabId)
		if (!tab) return
		const connectionId = tab.connectionId
		const siblingIdx = this.openTabs
			.filter((t) => t.connectionId === connectionId)
			.findIndex((t) => t.id === tabId)
		const wasActive = this.activeTabId === tabId
		this.openTabs = this.openTabs.filter((t) => t.id !== tabId)

		if (tab.kind === 'query') {
			const drafts = { ...this.queryDrafts }
			const baselines = { ...this.queryBaselines }
			delete drafts[tabId]
			delete baselines[tabId]
			this.queryRunCache.delete(tabId)
			this.queryDrafts = drafts
			this.queryBaselines = baselines
		}

		if (wasActive) {
			const remaining = this.openTabs.filter(
				(t) => t.connectionId === connectionId,
			)
			const next =
				remaining[siblingIdx] ??
				remaining[siblingIdx - 1] ??
				remaining[0] ??
				null
			this.activeConnectionId = connectionId
			this.activeTabId = next?.id ?? null
			this.rememberTab(connectionId, next?.id ?? null)
		} else if (this.activeTabByConnection[connectionId] === tabId) {
			const remaining = this.openTabs.filter(
				(t) => t.connectionId === connectionId,
			)
			this.rememberTab(connectionId, remaining[0]?.id ?? null)
		}
		if (this.selectedRow) {
			const active = this.openTabs.find((t) => t.id === this.activeTabId)
			if (
				!active ||
				active.kind !== 'table' ||
				active.connectionId !== this.selectedRow.connectionId ||
				active.tableId !== this.selectedRow.tableId
			) {
				this.selectedRow = null
			}
		}
		this.schedulePersist(connectionId)
	}

	setActiveTab(tabId: string) {
		const tab = this.openTabs.find((t) => t.id === tabId)
		if (!tab) return
		this.focusConnectionTab(tab.connectionId, tabId)
		if (
			this.selectedRow &&
			(tab.kind !== 'table' ||
				this.selectedRow.connectionId !== tab.connectionId ||
				this.selectedRow.tableId !== tab.tableId)
		) {
			this.selectedRow = null
		}
		this.schedulePersist(tab.connectionId)
	}

	/**
	 * Move a tab within its connection. `beforeId` is the tab it should sit
	 * in front of; `null` appends to the end of that connection's tabs.
	 */
	reorderTabs(movedId: string, beforeId: string | null) {
		if (movedId === beforeId) return
		const moved = this.openTabs.find((t) => t.id === movedId)
		if (!moved) return
		const connectionId = moved.connectionId
		if (
			beforeId &&
			!this.openTabs.some(
				(t) => t.id === beforeId && t.connectionId === connectionId,
			)
		) {
			return
		}

		const conn = this.openTabs.filter((t) => t.connectionId === connectionId)
		const from = conn.findIndex((t) => t.id === movedId)
		const dest =
			beforeId == null ? conn.length : conn.findIndex((t) => t.id === beforeId)
		if (from < 0 || dest < 0 || from === dest || from + 1 === dest) return

		const nextConn = conn.slice()
		const [item] = nextConn.splice(from, 1)
		const insertAt =
			beforeId == null
				? nextConn.length
				: nextConn.findIndex((t) => t.id === beforeId)
		if (insertAt < 0) return
		nextConn.splice(insertAt, 0, item)

		let i = 0
		this.openTabs = this.openTabs.map((t) =>
			t.connectionId === connectionId ? nextConn[i++]! : t,
		)
		this.schedulePersist(connectionId)
	}

	/**
	 * Move an open connection in the rail. `beforeId` is the session it should
	 * sit in front of; `null` appends to the end.
	 */
	reorderSessions(movedId: string, beforeId: string | null) {
		if (movedId === beforeId) return
		const from = this.sessions.findIndex((s) => s.profile.id === movedId)
		if (from < 0) return
		const dest =
			beforeId == null
				? this.sessions.length
				: this.sessions.findIndex((s) => s.profile.id === beforeId)
		if (dest < 0 || from === dest || from + 1 === dest) return

		const next = this.sessions.slice()
		const [item] = next.splice(from, 1)
		if (!item) return
		const insertAt =
			beforeId == null
				? next.length
				: next.findIndex((s) => s.profile.id === beforeId)
		if (insertAt < 0) return
		next.splice(insertAt, 0, item)
		this.sessions = next
	}

	getQueryRunCache(tabId: string): QueryRunCache | undefined {
		return this.queryRunCache.get(tabId)
	}

	setQueryRunCache(tabId: string, cache: QueryRunCache) {
		this.queryRunCache.set(tabId, cache)
	}

	setQueryDraft(tabId: string, text: string) {
		this.queryDrafts = { ...this.queryDrafts, [tabId]: text }
		const baseline = this.queryBaselines[tabId] ?? ''
		const dirty = text !== baseline
		const tab = this.openTabs.find((t) => t.id === tabId)
		if (tab && !!tab.dirty !== dirty) {
			this.openTabs = this.openTabs.map((t) =>
				t.id === tabId ? { ...t, dirty } : t,
			)
		}
		if (tab) this.schedulePersist(tab.connectionId)
	}

	/** After successful save: link savedQueryId, update title, clear dirty. */
	markQuerySaved(
		tabId: string,
		saved: { id: string; name: string; body: string },
	) {
		this.queryBaselines = { ...this.queryBaselines, [tabId]: saved.body }
		this.queryDrafts = { ...this.queryDrafts, [tabId]: saved.body }
		this.openTabs = this.openTabs.map((t) =>
			t.id === tabId
				? {
						...t,
						savedQueryId: saved.id,
						title: saved.name,
						dirty: false,
					}
				: t,
		)
		const tab = this.openTabs.find((t) => t.id === tabId)
		if (tab) this.schedulePersist(tab.connectionId)
	}

	/** Rename open tabs linked to a saved query (library rename). */
	renameSavedQueryTabs(savedQueryId: string, name: string) {
		const title = name.trim()
		if (!title) return
		let touchedConn: string | null = null
		this.openTabs = this.openTabs.map((t) => {
			if (t.kind === 'query' && t.savedQueryId === savedQueryId) {
				touchedConn = t.connectionId
				// Keep dirty if draft diverged; only rename chrome.
				return { ...t, title }
			}
			return t
		})
		if (touchedConn) this.schedulePersist(touchedConn)
	}

	/** Close open tabs linked to a deleted saved query. */
	closeTabsForSavedQuery(savedQueryId: string) {
		const ids = this.openTabs
			.filter((t) => t.kind === 'query' && t.savedQueryId === savedQueryId)
			.map((t) => t.id)
		for (const id of ids) this.closeTab(id)
	}

	/** Discard query draft back to last saved baseline and clear dirty. */
	discardQueryDraft(tabId: string) {
		const baseline = this.queryBaselines[tabId] ?? ''
		this.queryDrafts = { ...this.queryDrafts, [tabId]: baseline }
		this.openTabs = this.openTabs.map((t) =>
			t.id === tabId ? { ...t, dirty: false } : t,
		)
		const tab = this.openTabs.find((t) => t.id === tabId)
		if (tab) this.schedulePersist(tab.connectionId)
	}

	setTabDirty(tabId: string, dirty: boolean) {
		const tab = this.openTabs.find((t) => t.id === tabId)
		if (!tab || !!tab.dirty === dirty) return
		this.openTabs = this.openTabs.map((t) =>
			t.id === tabId ? { ...t, dirty } : t,
		)
		this.schedulePersist(tab.connectionId)
	}

	renameTab(tabId: string, title: string) {
		const name = title.trim()
		if (!name) return
		this.openTabs = this.openTabs.map((t) =>
			t.id === tabId ? { ...t, title: name } : t,
		)
		const tab = this.openTabs.find((t) => t.id === tabId)
		if (tab) this.schedulePersist(tab.connectionId)
	}

	selectRow(connectionId: string, tableId: string, rowIndex: number) {
		this.selectedRow = { connectionId, tableId, rowIndex }
	}

	clearRowSelection() {
		this.selectedRow = null
	}

	queueAgentPrompt(text: string) {
		const next = text.trim()
		if (!next) return
		this.pendingAgentPrompt = next
	}

	takePendingAgentPrompt(): string | null {
		const next = this.pendingAgentPrompt
		this.pendingAgentPrompt = null
		return next
	}
}

export const workspaceSession = new WorkspaceSessionStore()
