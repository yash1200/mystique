<script lang="ts">
	import { goto } from '$app/navigation'
	import { resolve } from '$app/paths'
	import type { ConnectionProfile, TableMeta } from '$core/contracts'
	import {
		closeEngineSession,
		ensureEngineSession,
		isTauri,
		listenNativeMenu,
		setNativeMenu,
		testEngineConnection,
	} from '$core/ipc'
	import { setAppTitle } from '$core/util'
	import {
		colorHex,
		connectionProfiles,
		draftFromProfile,
		pluginConfigForSession,
	} from '$features/connections'
	import {
		catalogColumnsCache,
		ensureCatalogForActiveSession,
		ensureLiveCatalog,
	} from '$features/workspace/catalog'
	import { GridTransferHost, tablePreview } from '$features/workspace/grid'
	import {
		querySave,
		QuerySaveHost,
		refreshHistory,
		refreshSavedQueries,
	} from '$features/workspace/query'
	import { onDestroy, onMount } from 'svelte'
	import { reopenEngineSession, sessionDatabase } from '../live-session'
	import { workspaceSession } from '../session.svelte'
	import { mountWorkspaceShortcuts } from '../shortcuts'
	import { workspaceStudio } from '../studio.svelte'
	import OpenConnectionModal from './OpenConnectionModal.svelte'
	import type { SideNavMode } from './SessionsRail.svelte'
	import SessionsRail from './SessionsRail.svelte'
	import UnsavedCloseModal from './UnsavedCloseModal.svelte'
	import WorkspaceMainPane from './WorkspaceMainPane.svelte'
	import WorkspaceSidePane from './WorkspaceSidePane.svelte'

	const sessions = $derived(workspaceSession.sessions)
	const activeId = $derived(workspaceSession.activeConnectionId)
	const activeSession = $derived(workspaceSession.activeSession)

	$effect(() => {
		void setAppTitle(activeSession?.profile.name ?? null)
	})
	onDestroy(() => {
		void setAppTitle()
	})

	$effect(() => {
		void activeSession?.profile.id
		void activeSession?.engineSessionId
		void ensureCatalogForActiveSession()
	})

	let pendingCloseTabId = $state<string | null>(null)
	let closePromptBusy = $state(false)
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

	async function confirmSaveAndClose() {
		const tab = pendingCloseTab
		if (!tab) return
		closePromptBusy = true
		try {
			if (tab.kind === 'query') {
				const ok = await querySave.saveTab(tab.id)
				if (!ok) return
				if (workspaceSession.openTabs.some((t) => t.id === tab.id)) {
					workspaceSession.closeTab(tab.id)
				}
				finishQueuedClose(tab.id)
			} else {
				workspaceSession.setTabDirty(tab.id, false)
				workspaceSession.closeTab(tab.id)
				finishQueuedClose(tab.id)
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
						await tablePreview.open(table)
					} catch (e) {
						console.warn('[mystique] discard table reload:', e)
					}
				}
				workspaceSession.setTabDirty(tab.id, false)
				workspaceStudio.baselineEpoch += 1
			} else {
				workspaceSession.setTabDirty(tab.id, false)
			}
			workspaceSession.closeTab(tab.id)
			finishQueuedClose(tab.id)
		} finally {
			closePromptBusy = false
		}
	}

	let pickerOpen = $state(false)
	let pickerBusy = $state(false)
	let pickerError = $state<string | null>(null)
	const openConnectionIds = $derived(sessions.map((s) => s.profile.id))

	function onSelectConnection(id: string) {
		workspaceStudio.sideMode = 'explorer'
		workspaceSession.setActiveConnection(id)
	}

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
		const session = workspaceSession.sessions.find((s) => s.profile.id === id)
		if (!session) return
		const engine = String(session.profile.engine)
		const preferred = sessionDatabase(id, session.profile)
		if (!isTauri()) return
		workspaceStudio.liveCatalogKey = {
			...workspaceStudio.liveCatalogKey,
			[id]: '',
		}
		const sid = await reopenEngineSession(session.profile, preferred)
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
		} else if (mode === 'history') {
			workspaceStudio.selectedQueryId = null
			void refreshHistory({ preferSelectLatest: true })
		}
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
			void querySave.saveActive()
			return
		}
		if (tab.kind === 'table') {
			window.dispatchEvent(new Event('mystique:save-table'))
		}
	}

	$effect(() => {
		setNativeMenu({
			page: 'workspace',
			hasSession: !!activeId,
			hasTab: !!workspaceSession.activeTabId,
		})
	})

	onMount(() => {
		const openPicker = () => {
			pickerError = null
			pickerOpen = true
		}
		const unsubKeys = mountWorkspaceShortcuts({
			onNewQuery: shortcutNewQuery,
			onCloseTab: shortcutCloseTab,
			onSaveQuery: shortcutSave,
			onOpenConnection: openPicker,
			isTauri: isTauri(),
		})
		let unsubMenu = () => {}
		let cancelled = false
		void listenNativeMenu({
			'workspace-connection-details': () => {
				window.dispatchEvent(new Event('mystique:connection-details'))
			},
			'workspace-copy-url': () => {
				window.dispatchEvent(new Event('mystique:copy-connection-url'))
			},
			'workspace-refresh-catalog': () => {
				if (activeId) void refreshConnection(activeId)
			},
			'workspace-disconnect': () => {
				if (activeId) handleDisconnect(activeId)
			},
			'workspace-go-home': () => {
				window.dispatchEvent(new Event('mystique:go-home'))
			},
			'workspace-view-explorer': () => setSideMode('explorer'),
			'workspace-view-saved': () => setSideMode('saved-queries'),
			'workspace-view-history': () => setSideMode('history'),
		}).then((u) => {
			if (cancelled) u()
			else unsubMenu = u
		})
		return () => {
			cancelled = true
			unsubKeys()
			unsubMenu()
		}
	})
</script>

<div
	class="flex h-screen w-screen overflow-hidden bg-app"
	style={activeSession
		? `--connection-color: ${colorHex(activeSession.profile.colorId)}`
		: undefined}
	data-env={activeSession?.profile.environment ?? undefined}
>
	<SessionsRail
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

	<WorkspaceSidePane />

	<WorkspaceMainPane
		onclosetab={requestCloseTab}
		onclosemany={enqueueCloseTabs}
	/>

	<GridTransferHost />
	<QuerySaveHost />

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
		open={!!pendingCloseTab && !querySave.modalOpen}
		tabTitle={pendingCloseTab?.title ?? 'Tab'}
		tabKind={pendingCloseTab?.kind ?? 'query'}
		busy={closePromptBusy || workspaceStudio.querySaving}
		onsave={() => void confirmSaveAndClose()}
		ondiscard={() => void confirmDiscardAndClose()}
		oncancel={cancelCloseTab}
	/>
</div>
