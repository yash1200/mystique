<script lang="ts">
	import { goto } from '$app/navigation'
	import { resolve } from '$app/paths'
	import type {
		ConnectionProfile,
		SshHostKeyPrompt,
		TableMeta,
	} from '#core/contracts'
	import {
		errorMessage,
		closeEngineSession,
		commitPinnedTab,
		isTauri,
		rollbackPinnedTab,
		setNativeMenu,
		sshHostKeyPrompt,
	} from '#core/ipc'
	import { commands } from '#core/commands'
	import { closeTopDialog } from '#core/ui'
	import { setAppTitle } from '#core/util'
	import {
		acceptSshHostKey,
		colorHex,
		connectionProfiles,
		ensureSessionWithPassword,
		isConnectCancelled,
		pluginConfigForSession,
		SshHostKeyModal,
		testSessionWithPassword,
	} from '#features/connections'
	import { appChrome, cmd } from '#lib/app'
	import {
		catalogColumnsCache,
		ensureCatalogForActiveSession,
		ensureLiveCatalog,
	} from '#features/workspace/catalog'
	import { GridTransferHost, tablePreview } from '#features/workspace/grid'
	import {
		querySave,
		QuerySaveHost,
		refreshHistory,
		refreshSavedQueries,
	} from '#features/workspace/query'
	import { onDestroy, onMount, untrack } from 'svelte'
	import {
		probeSessionHealth,
		reopenEngineSession,
		sessionDatabase,
		startSessionHealthWatch,
		closeStep,
		saveThenClose,
		dropEditBuffer,
		saveTableTab,
		workspaceSession,
		workspaceStudio,
	} from '#features/workspace/session'
	import OpenConnectionModal from './OpenConnectionModal.svelte'
	import OpenTransactionModal from './OpenTransactionModal.svelte'
	import type { SideNavMode } from './SessionsRail.svelte'
	import SessionsRail from './SessionsRail.svelte'
	import UnsavedCloseModal from './UnsavedCloseModal.svelte'
	import WriteGuardModal from './WriteGuardModal.svelte'
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
		const connectionId = activeSession?.profile.id
		const engineSessionId = activeSession?.engineSessionId
		if (!connectionId) return
		void engineSessionId
		untrack(() => {
			void ensureCatalogForActiveSession()
		})
	})

	$effect(() => {
		const id = activeSession?.profile.id
		void activeSession?.engineSessionId
		if (id) void probeSessionHealth(id)
	})

	let pendingCloseTabId = $state<string | null>(null)
	let closePromptBusy = $state(false)
	/** The grid's own save dialogs are up; hide the close prompt meanwhile. */
	let closeTableSaving = $state(false)
	let closeError = $state<string | null>(null)
	let closeQueue = $state<string[]>([])
	const pendingCloseTab = $derived(
		pendingCloseTabId
			? (workspaceSession.openTabs.find((t) => t.id === pendingCloseTabId) ??
					null)
			: null,
	)
	const pendingCloseStep = $derived(
		pendingCloseTab
			? closeStep(pendingCloseTab, workspaceSession.tabTxn[pendingCloseTab.id])
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
			if (closeStep(tab, workspaceSession.tabTxnOf(id)) !== 'close') {
				workspaceSession.setActiveTab(id)
				closeError = null
				pendingCloseTabId = id
				return
			}
			workspaceSession.closeTab(id)
			closeQueue = closeQueue.slice(1)
		}
	}

	function finishQueuedClose(tabId: string) {
		pendingCloseTabId = null
		closeError = null
		closeQueue = closeQueue.filter((id) => id !== tabId)
		drainCloseQueue()
	}

	function cancelCloseTab() {
		if (closePromptBusy) return
		pendingCloseTabId = null
		closeError = null
		closeQueue = []
	}

	function closeNow(tabId: string) {
		if (workspaceSession.openTabs.some((t) => t.id === tabId)) {
			workspaceSession.closeTab(tabId)
		}
		finishQueuedClose(tabId)
	}

	async function resolveTransaction(action: 'commit' | 'rollback') {
		const tab = pendingCloseTab
		if (!tab) return
		closePromptBusy = true
		closeError = null
		try {
			const status =
				action === 'commit'
					? await commitPinnedTab(tab.id)
					: await rollbackPinnedTab(tab.id)
			workspaceSession.setTabTxn(tab.id, status)
			if (status.inTransaction) {
				closeError = 'The transaction is still open.'
				return
			}
			// Unsaved query text is asked about next; otherwise close.
			if (closeStep(tab, status) === 'close') closeNow(tab.id)
		} catch (e) {
			closeError = errorMessage(e)
		} finally {
			closePromptBusy = false
		}
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
			} else if (tab.kind === 'table') {
				// The grid runs its own save (preview, write guard). Close only
				// once it committed; a failure keeps the tab and shows why.
				closeError = null
				closeTableSaving = true
				const outcome = await saveThenClose(
					() => saveTableTab(tab.id),
					() => closeNow(tab.id),
				)
				closeTableSaving = false
				if (!outcome.ok) closeError = outcome.error
			} else {
				workspaceSession.setTabDirty(tab.id, false)
				workspaceSession.closeTab(tab.id)
				finishQueuedClose(tab.id)
			}
		} finally {
			closeTableSaving = false
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
						// Closing discards the edits. Drop them first so the
						// reload is allowed to replace the page.
						dropEditBuffer(tab.id)
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
	let sshHostKey = $state<SshHostKeyPrompt | null>(null)
	let sshHostKeyRetry = $state<null | (() => Promise<void>)>(null)
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
			const pluginConfig = pluginConfigForSession(profile)
			const result = await testSessionWithPassword(
				{
					engine: String(profile.engine),
					pluginConfig,
					profileId: profile.id,
					pool: { maxSize: 2, minIdle: 0, acquireTimeoutMs: 8000 },
				},
				profile.name,
			)
			if (result.sshHostKey) {
				sshHostKey = result.sshHostKey
				sshHostKeyRetry = () => connectAdditional(profile)
				return
			}
			if (!result.ok) {
				pickerError = `Could not connect: ${result.message}`
				return
			}
			const updated = await connectionProfiles.touchConnected(profile.id)
			const session = await ensureSessionWithPassword(
				{
					engine: String(profile.engine),
					pluginConfig,
					profileId: profile.id,
					pool: { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 },
					sessionId: profile.id,
				},
				profile.name,
			)
			workspaceSession.connect(
				{ ...updated, status: 'connected' },
				session.sessionId,
				session.capabilities,
			)
			workspaceStudio.sideMode = 'explorer'
			pickerOpen = false
		} catch (e) {
			if (isConnectCancelled(e)) return
			const prompt = sshHostKeyPrompt(e)
			if (prompt) {
				sshHostKey = prompt
				sshHostKeyRetry = () => connectAdditional(profile)
				return
			}
			pickerError = errorMessage(e)
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
		// ⌘W closes the top dialog first.
		if (closeTopDialog()) return
		const id = workspaceSession.activeTabId
		if (!id) return
		requestCloseTab(id)
	}

	$effect(() => {
		setNativeMenu({
			page: 'workspace',
			hasSession: !!activeId,
			hasTab: !!workspaceSession.activeTabId,
			hasClosedTab: workspaceSession.closedTabs.length > 0,
		})
	})

	function quickConnect(id: string) {
		if (workspaceSession.sessions.some((session) => session.profile.id === id)) {
			workspaceSession.setActiveConnection(id)
			return
		}
		const profile = connectionProfiles.profile(id)
		if (profile) void connectAdditional(profile)
	}

	onMount(() => {
		const openPicker = () => {
			pickerError = null
			pickerOpen = true
		}
		const unregister = commands.register([
			{ ...cmd.newQuery, run: shortcutNewQuery },
			{ ...cmd.closeTab, run: shortcutCloseTab },
			{ ...cmd.reopenTab, run: () => void workspaceSession.reopenClosedTab() },
			// A table tab's grid registers its own save while it is mounted.
			{
				...cmd.save,
				when: () => workspaceSession.activeTab?.kind === 'query',
				run: () => void querySave.saveActive(),
			},
			{ ...cmd.openConnection, run: openPicker },
			{
				...cmd.refreshCatalog,
				run: () => {
					if (activeId) void refreshConnection(activeId)
				},
			},
			{
				...cmd.disconnect,
				run: () => {
					if (activeId) handleDisconnect(activeId)
				},
			},
			{ ...cmd.viewExplorer, run: () => setSideMode('explorer') },
			{ ...cmd.viewSaved, run: () => setSideMode('saved-queries') },
			{ ...cmd.viewHistory, run: () => setSideMode('history') },
			{
				...cmd.openConnectionById,
				run: ({ args }) => {
					if (args?.id) quickConnect(args.id)
				},
			},
		])
		const unsubHealth = startSessionHealthWatch()

		return () => {
			unregister()
			unsubHealth()
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
	{#if appChrome.sessionRailOpen}
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
	{/if}

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

	<WriteGuardModal />

	<OpenTransactionModal
		open={pendingCloseStep === 'transaction'}
		tabTitle={pendingCloseTab?.title ?? 'Tab'}
		busy={closePromptBusy}
		error={closeError}
		oncommit={() => void resolveTransaction('commit')}
		onrollback={() => void resolveTransaction('rollback')}
		oncancel={cancelCloseTab}
	/>

	<UnsavedCloseModal
		open={pendingCloseStep === 'unsaved' &&
			!querySave.modalOpen &&
			!closeTableSaving}
		tabTitle={pendingCloseTab?.title ?? 'Tab'}
		tabKind={pendingCloseTab?.kind ?? 'query'}
		error={closeError}
		busy={closePromptBusy || workspaceStudio.querySaving}
		onsave={() => void confirmSaveAndClose()}
		ondiscard={() => void confirmDiscardAndClose()}
		oncancel={cancelCloseTab}
	/>

	<SshHostKeyModal
		prompt={sshHostKey}
		busy={pickerBusy}
		oncancel={() => {
			sshHostKey = null
			sshHostKeyRetry = null
		}}
		onaccept={async (prompt) => {
			try {
				await acceptSshHostKey(prompt)
				const retry = sshHostKeyRetry
				sshHostKey = null
				sshHostKeyRetry = null
				await retry?.()
			} catch (e) {
				pickerError = errorMessage(e)
			}
		}}
	/>
</div>
