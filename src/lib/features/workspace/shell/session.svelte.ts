/**
 * Connected-session state for the workspace shell.
 * Open tabs + query drafts persist to the app SQLite DB (via Tauri IPC).
 */
import type {
	ConnectedSession,
	ConnectionProfile,
	EngineCapabilities,
	SessionLiveStatus,
} from '#core/contracts'
import { cancelTabRun, isTauri, unpinQueryTab } from '#core/ipc'
import { normalizeCapabilities } from '#engines'
import type { QueryRunCache } from '#features/workspace/query'
import {
	listWorkspaceTabs,
	recordToWorkspaceTab,
	replaceWorkspaceTabs,
	upsertWorkspaceTab,
	workspaceTabToSaveInput,
} from './api'
import type { TabTxn } from './close-guard'
import {
	closedTabEntry,
	forgetConnection,
	rememberClosed,
	reopenTab,
	takeClosed,
	type ClosedTab,
} from './closed-tabs'
import { tabDraftUpsert } from './tab-persist'
import { dropEditBuffer } from './edit-buffer'
import { createPersistScheduler } from './persist-schedule'
import type { SelectedRow, TableTabPreview, WorkspaceTab } from './types'

export type { ConnectedSession } from '#core/contracts'

export function tableTabId(connectionId: string, tableId: string): string {
	return `${connectionId}::table::${tableId}`
}

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
	 * Query run per tab, including an in-flight one. The Map stays plain so
	 * result rows are not proxied. `queryRunVersion` is the only reactive bit.
	 */
	private queryRunCache = new Map<string, QueryRunCache>()
	private queryRunVersion = $state(0)
	/**
	 * Manual-commit state per query tab. Kept here, not in the canvas, so a
	 * tab switch reads the right state before the async status returns.
	 */
	tabTxn = $state<Record<string, TabTxn>>({})
	/** Recently closed tabs for ⌘⇧T. In memory only. */
	closedTabs = $state<ClosedTab[]>([])

	/** Debounced persist handles per connection. */
	private persistScheduler = createPersistScheduler((connectionId) =>
		this.persistNow(connectionId),
	)
	/** One timer per tab. A keystroke upserts that tab, not the whole list. */
	private draftScheduler = createPersistScheduler((tabId) => {
		void this.persistDraft(tabId)
	})
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
			liveStatus: 'connected',
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
		this.cancelDrafts(connectionId)
		this.persistScheduler.schedule(connectionId)
	}

	private cancelDrafts(connectionId: string) {
		this.draftScheduler.cancelWhere((tabId) =>
			this.openTabs.some(
				(tab) => tab.id === tabId && tab.connectionId === connectionId,
			),
		)
	}

	private scheduleDraft(tabId: string) {
		const tab = this.openTabs.find((item) => item.id === tabId)
		if (!tab || !isTauri() || this.restoring.has(tab.connectionId)) return
		if (this.persistScheduler.pending(tab.connectionId)) return
		this.draftScheduler.schedule(tabId)
	}

	private activeTabFor(connectionId: string): string | null {
		const tabs = this.openTabs.filter((tab) => tab.connectionId === connectionId)
		const remembered = this.activeTabByConnection[connectionId]
		if (remembered && tabs.some((tab) => tab.id === remembered)) return remembered
		if (this.activeTabId && tabs.some((tab) => tab.id === this.activeTabId)) {
			return this.activeTabId
		}
		return tabs[0]?.id ?? null
	}

	/** Keystroke path: one row, one body. */
	private async persistDraft(tabId: string) {
		const tab = this.openTabs.find((item) => item.id === tabId)
		if (!tab || !isTauri() || this.restoring.has(tab.connectionId)) return
		const tabs = this.openTabs.filter(
			(item) => item.connectionId === tab.connectionId,
		)
		const input = tabDraftUpsert(
			tabs,
			tabId,
			this.queryDrafts[tabId] ?? '',
			this.activeTabFor(tab.connectionId),
		)
		if (!input) return
		try {
			await upsertWorkspaceTab(input)
		} catch (e) {
			console.warn('[mystique] persist query draft:', e)
		}
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

	setLiveStatus(id: string, liveStatus: SessionLiveStatus) {
		const i = this.sessions.findIndex((s) => s.profile.id === id)
		if (i < 0) return
		if (this.sessions[i]?.liveStatus === liveStatus) return
		this.sessions = this.sessions.map((s, idx) =>
			idx === i ? { ...s, liveStatus } : s,
		)
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
		// A pending timer would otherwise fire after the tab list is cleared
		// and persist an empty set. Draft timers are tab-keyed, so cancel those too.
		this.persistScheduler.cancel(id)
		this.cancelDrafts(id)
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
			void cancelTabRun(tid)
		}
		this.dropTabTxn(removedTabIds)
		this.closedTabs = forgetConnection(this.closedTabs, id)
		this.queryRunVersion += 1
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
		const id = tableTabId(connectionId, tableId)
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

	/** `remember: false` keeps it out of ⌘⇧T (its saved query is gone). */
	closeTab(tabId: string, opts: { remember?: boolean } = {}) {
		void cancelTabRun(tabId)
		const tab = this.openTabs.find((t) => t.id === tabId)
		if (isTauri() && tab?.kind === 'query') {
			void unpinQueryTab(tabId).catch(() => {})
		}
		if (!tab) return
		if (opts.remember !== false) {
			const entry = closedTabEntry(
				tab,
				this.openTabs.indexOf(tab),
				this.queryDrafts,
				this.queryBaselines,
			)
			this.closedTabs = rememberClosed(this.closedTabs, entry)
		}
		const connectionId = tab.connectionId
		const siblingIdx = this.openTabs
			.filter((t) => t.connectionId === connectionId)
			.findIndex((t) => t.id === tabId)
		const wasActive = this.activeTabId === tabId
		this.openTabs = this.openTabs.filter((t) => t.id !== tabId)
		if (tab.kind === 'table') dropEditBuffer(tabId)
		this.dropTabTxn([tabId])

		if (tab.kind === 'query') {
			const drafts = { ...this.queryDrafts }
			const baselines = { ...this.queryBaselines }
			delete drafts[tabId]
			delete baselines[tabId]
			this.queryRunCache.delete(tabId)
			this.queryRunVersion += 1
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

	/** ⌘⇧T: bring back the last closed tab with its kind, target and text. */
	reopenClosedTab(): boolean {
		const { entry, rest } = takeClosed(
			this.closedTabs,
			(id) => this.sessions.some((s) => s.profile.id === id),
			(id) => this.openTabs.some((t) => t.id === id),
		)
		this.closedTabs = rest
		if (!entry) return false
		const { tab, openTabs } = reopenTab(this.openTabs, entry)
		this.openTabs = openTabs
		if (tab.kind === 'query') {
			this.queryDrafts = { ...this.queryDrafts, [tab.id]: entry.text ?? '' }
			this.queryBaselines = {
				...this.queryBaselines,
				[tab.id]: entry.baseline ?? '',
			}
		}
		this.focusConnectionTab(tab.connectionId, tab.id)
		this.selectedRow = null
		this.schedulePersist(tab.connectionId)
		return true
	}

	tabTxnOf(tabId: string): TabTxn | undefined {
		return this.tabTxn[tabId]
	}

	setTabTxn(tabId: string, status: TabTxn) {
		const prev = this.tabTxn[tabId]
		if (
			prev?.pinned === status.pinned &&
			prev?.inTransaction === status.inTransaction
		) {
			return
		}
		this.tabTxn = {
			...this.tabTxn,
			[tabId]: { pinned: status.pinned, inTransaction: status.inTransaction },
		}
	}

	private dropTabTxn(tabIds: string[]) {
		if (!tabIds.some((id) => id in this.tabTxn)) return
		const next = { ...this.tabTxn }
		for (const id of tabIds) delete next[id]
		this.tabTxn = next
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
		void this.queryRunVersion
		return this.queryRunCache.get(tabId)
	}

	hasOpenTab(tabId: string): boolean {
		return this.openTabs.some((tab) => tab.id === tabId)
	}

	setQueryRunCache(tabId: string, cache: QueryRunCache) {
		this.queryRunCache.set(tabId, cache)
		this.queryRunVersion += 1
	}

	setQueryDraft(tabId: string, text: string) {
		if ((this.queryDrafts[tabId] ?? '') === text) {
			if (this.queryDrafts[tabId] === undefined) this.queryDrafts[tabId] = text
			return
		}
		this.queryDrafts[tabId] = text
		const baseline = this.queryBaselines[tabId] ?? ''
		const dirty = text !== baseline
		const tab = this.openTabs.find((t) => t.id === tabId)
		if (tab && !!tab.dirty !== dirty) {
			this.openTabs = this.openTabs.map((t) =>
				t.id === tabId ? { ...t, dirty } : t,
			)
		}
		if (tab) this.scheduleDraft(tabId)
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
		for (const id of ids) this.closeTab(id, { remember: false })
		this.closedTabs = this.closedTabs.filter(
			(item) => item.tab.savedQueryId !== savedQueryId,
		)
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

	/** In-memory table preview. Not persisted — closing the tab drops it. */
	setTablePreview(tabId: string, preview: TableTabPreview) {
		const tab = this.openTabs.find((t) => t.id === tabId)
		if (!tab || tab.kind !== 'table') return
		this.openTabs = this.openTabs.map((t) =>
			t.id === tabId ? { ...t, preview } : t,
		)
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
