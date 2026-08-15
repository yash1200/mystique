/**
 * Connected-session state for the workspace shell.
 * Open tabs + query drafts persist to the app SQLite DB (via Tauri IPC).
 */
import { isTauri } from '$lib/api/tauri'
import {
	listWorkspaceTabs,
	recordToWorkspaceTab,
	replaceWorkspaceTabs,
	workspaceTabToSaveInput,
} from '$lib/api/workspaceTabs'
import type { ConnectionProfile } from '$lib/types/connection'
import {
	normalizeCapabilities,
	type EngineCapabilities,
} from '$lib/types/engineCaps'
import type { SelectedRow, WorkspaceTab } from '$lib/types/workspace'

export type ConnectedSession = {
	profile: ConnectionProfile
	connectedAt: string
	/** Live engine pool session (Tauri ConnectionManager). */
	engineSessionId?: string | null
	/** Plugin-owned feature config for this live session. */
	capabilities?: EngineCapabilities | null
}

class WorkspaceSessionStore {
	sessions = $state<ConnectedSession[]>([])
	activeConnectionId = $state<string | null>(null)
	openTabs = $state<WorkspaceTab[]>([])
	activeTabId = $state<string | null>(null)
	selectedRow = $state<SelectedRow | null>(null)

	/** Draft SQL/MQL text per query tab id. */
	queryDrafts = $state<Record<string, string>>({})
	/** Baseline body used to compute dirty (saved body or last restore). */
	queryBaselines = $state<Record<string, string>>({})
	/** Allow-writes flag per query tab id. */
	queryWriteMode = $state<Record<string, boolean>>({})

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
			const writes: Record<string, boolean> = { ...this.queryWriteMode }

			for (const r of records) {
				if (r.kind === 'query') {
					const body = r.body ?? ''
					drafts[r.id] = body
					baselines[r.id] = body
					writes[r.id] = !!r.writeEnabled
				}
			}

			this.openTabs = [...this.openTabs, ...tabs]
			this.queryDrafts = drafts
			this.queryBaselines = baselines
			this.queryWriteMode = writes

			const active =
				records.find((r) => r.isActive)?.id ??
				records[records.length - 1]?.id ??
				null
			if (active && this.activeConnectionId === connectionId) {
				this.activeTabId = active
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
		const activeTabId =
			this.activeTabId && tabs.some((t) => t.id === this.activeTabId)
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
						writeEnabled: this.queryWriteMode[tab.id] ?? false,
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
			this.activeTabId = existing.id
			this.activeConnectionId = connectionId
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
		const title = count === 0 ? 'Untitled query' : `Untitled query ${count + 1}`
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
		this.queryWriteMode = { ...this.queryWriteMode, [id]: false }
		this.activeTabId = id
		this.activeConnectionId = connectionId
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
			this.activeTabId = existing.id
			this.activeConnectionId = connectionId
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
		this.queryWriteMode = { ...this.queryWriteMode, [id]: false }
		this.activeTabId = id
		this.activeConnectionId = connectionId
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
		this.activeConnectionId = id
		if (this.selectedRow && this.selectedRow.connectionId !== id) {
			this.selectedRow = null
		}
		const tab =
			this.openTabs.find(
				(t) => t.connectionId === id && t.id === this.activeTabId,
			) ?? this.openTabs.find((t) => t.connectionId === id)
		this.activeTabId = tab?.id ?? null
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
		const writes = { ...this.queryWriteMode }
		for (const tid of removedTabIds) {
			delete drafts[tid]
			delete baselines[tid]
			delete writes[tid]
		}
		this.queryDrafts = drafts
		this.queryBaselines = baselines
		this.queryWriteMode = writes

		if (this.activeConnectionId === id) {
			this.activeConnectionId = this.sessions[0]?.profile.id ?? null
			if (this.activeConnectionId) {
				const tab = this.openTabs.find(
					(t) => t.connectionId === this.activeConnectionId,
				)
				this.activeTabId = tab?.id ?? null
			} else {
				this.activeTabId = null
			}
		}
		if (this.selectedRow?.connectionId === id) {
			this.selectedRow = null
		}
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
		this.activeTabId = id
		this.activeConnectionId = connectionId
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
		this.activeTabId = id
		this.activeConnectionId = connectionId
		this.selectedRow = null
		this.schedulePersist(connectionId)
	}

	/** Open / focus live ER · Model diagram tab for a connection (+ optional database). */
	openModel(connectionId: string, database: string, title = 'ER diagram') {
		const dbKey = database?.trim() || '_'
		const id = `${connectionId}::model::${dbKey}`
		if (!this.openTabs.some((t) => t.id === id)) {
			this.openTabs = [
				...this.openTabs,
				{
					id,
					connectionId,
					kind: 'model',
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
		this.activeTabId = id
		this.activeConnectionId = connectionId
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
			const writes = { ...this.queryWriteMode }
			delete drafts[tabId]
			delete baselines[tabId]
			delete writes[tabId]
			this.queryDrafts = drafts
			this.queryBaselines = baselines
			this.queryWriteMode = writes
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
			this.activeTabId = next?.id ?? null
			this.activeConnectionId = connectionId
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
		this.activeTabId = tabId
		this.activeConnectionId = tab.connectionId
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

	setQueryWriteMode(tabId: string, enabled: boolean) {
		this.queryWriteMode = { ...this.queryWriteMode, [tabId]: enabled }
		const tab = this.openTabs.find((t) => t.id === tabId)
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
}

export const workspaceSession = new WorkspaceSessionStore()
