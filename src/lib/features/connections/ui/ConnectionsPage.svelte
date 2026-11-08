<script lang="ts">
	import { goto } from '$app/navigation'
	import type {
		ConnectionFormDraft,
		ConnectionProfile,
		SshHostKeyPrompt,
	} from '#core/contracts'
	import {
		formatEngineConnectionUrl,
		isTauri,
		listenNativeMenu,
		setNativeMenu,
		testEngineConnection,
	} from '#core/ipc'
	import { Button, EmptyState, Icon, Modal } from '#core/ui'
	import { copyTextToClipboard, setAppTitle } from '#core/util'
	import { showToast } from '#features/toast'
	import { workspaceSession } from '#features/workspace/session'
	import { appChrome } from '#lib/app'
	import { onMount } from 'svelte'
	import {
		draftFromProfile,
		pluginConfigForSession,
		pluginConfigFromDraft,
		saveInputFromDraft,
	} from '../model'
	import {
		ensureSessionWithPassword,
		testSessionWithPassword,
	} from '../password-prompt.svelte'
	import { isConnectCancelled } from '../password-prompt'
	import { connectionProfiles } from '../profiles.svelte'
	import ConnectionDetail from './ConnectionDetail.svelte'
	import type { ConnectionMenuAction } from './ConnectionSidebar.svelte'
	import ConnectionSidebar from './ConnectionSidebar.svelte'
	import HomeIllustration from './HomeIllustration.svelte'
	import NewConnectionPanel from './NewConnectionPanel.svelte'
	import SshHostKeyModal from './SshHostKeyModal.svelte'
	import {
		acceptSshHostKey,
		invokeErrorMessage,
		parseSshHostKey,
		prettifyConnectError,
		sshTunnelFromDraft,
	} from '../ssh'

	let selectedId = $state<string | null>(null)
	/** Transient connect-failure overlay; live connected comes from the session store. */
	let statusOverride = $state<Record<string, 'error'>>({})
	const connections = $derived(
		connectionProfiles.items.map((p) => ({
			...p,
			status:
				statusOverride[p.id] === 'error'
					? ('error' as const)
					: workspaceSession.sessions.some((s) => s.profile.id === p.id)
						? ('connected' as const)
						: ('disconnected' as const),
		})),
	)
	/** Create form is the home landing (no selection). */
	let creating = $state(true)
	/** Edit form for the currently selected profile. */
	let editing = $state(false)
	/** Remount the create form to reset fields (Home / New / Cancel). */
	let createFormKey = $state(0)
	let loading = $state(true)
	let listRefreshing = $state(false)
	let saving = $state(false)
	let testing = $state(false)
	let connectingId = $state<string | null>(null)
	/** Last live connectivity result from BE (shown on the form). */
	let testResult = $state<{ ok: boolean; message: string } | null>(null)
	let sshHostKey = $state<SshHostKeyPrompt | null>(null)
	let sshHostKeyRetry = $state<null | (() => Promise<void>)>(null)
	let loadError = $state<string | null>(null)

	const selected = $derived(
		connections.find((c) => c.id === selectedId) ?? null,
	)
	/** The form shows `testResult` inline; the detail view does not. */
	const formShown = $derived(editing || !selected)

	/** Set the result banner; toast only when no banner is on screen. */
	function reportTestResult(ok: boolean, message: string, toast = message) {
		testResult = { ok, message }
		if (!formShown) showToast(ok ? 'success' : 'danger', toast)
	}

	/** Landing: create-connection form. Connections live in the sidebar. */
	function enterCreateMode() {
		selectedId = null
		creating = true
		editing = false
		testResult = null
		createFormKey += 1
	}

	function afterProfilesChange(preferSelectId?: string | null) {
		const rows = connectionProfiles.items
		if (preferSelectId && rows.some((c) => c.id === preferSelectId)) {
			selectedId = preferSelectId
			creating = false
			editing = false
		} else if (!selectedId || !rows.some((c) => c.id === selectedId)) {
			if (!creating && !editing) {
				enterCreateMode()
			}
		}
	}

	async function refreshList(preferSelectId?: string | null) {
		await connectionProfiles.refresh()
		afterProfilesChange(preferSelectId)
	}

	async function refreshConnections() {
		if (listRefreshing) return
		listRefreshing = true
		try {
			await refreshList()
		} catch (err) {
			showToast('danger', err instanceof Error ? err.message : String(err))
		} finally {
			listRefreshing = false
		}
	}

	function selectConnection(id: string) {
		creating = false
		editing = false
		testResult = null
		selectedId = id
	}

	function startCreate() {
		enterCreateMode()
	}

	function openImportUrl() {
		if (!creating || editing) enterCreateMode()
		window.setTimeout(() => {
			window.dispatchEvent(new Event('mystique:import-connection-url'))
		}, 0)
	}

	$effect(() => {
		setNativeMenu({
			page: 'home',
			hasSelection: selected != null,
		})
	})

	function overlayBlocksEscape(): boolean {
		return !!document.querySelector('[role="dialog"], [role="menu"]')
	}

	/** Esc on a selected connection returns to the new-connection landing. */
	function onHomeKeydown(e: KeyboardEvent) {
		if (e.key !== 'Escape' || e.repeat || e.isComposing) return
		if (e.defaultPrevented) return
		if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return
		if (creating || editing || !selected) return
		if (pendingDelete || overlayBlocksEscape()) return
		e.preventDefault()
		enterCreateMode()
	}

	function startEdit() {
		if (!selected) return
		editConnection(selected.id)
	}

	function editConnection(id: string) {
		const profile = connections.find((c) => c.id === id)
		if (!profile) return
		selectedId = id
		creating = false
		editing = true
		testResult = null
	}

	/** Live connectivity check via backend engine plugins. */
	async function handleTestConnection(draft: ConnectionFormDraft) {
		testResult = null

		if (!isTauri()) {
			reportTestResult(
				false,
				'Backend not available. Open the Mystique desktop app to test a real database connection.',
			)
			return
		}

		testing = true
		try {
			const pluginConfig = pluginConfigFromDraft(draft)
			const typed = draft.password.trim()
			const password = typed.length > 0 ? typed : null

			const tlsPass = (draft.sslClientKeyPassphrase ?? '').trim()
			const result = await testEngineConnection({
				engine: String(draft.engine),
				pluginConfig,
				password,
				profileId:
					editing && selectedId ? selectedId : undefined,
				ssh: sshTunnelFromDraft(draft),
				useSavedSsh: false,
				tlsKeyPassphrase: tlsPass || null,
				pool: {
					maxSize: 2,
					minIdle: 0,
					acquireTimeoutMs: 8000,
					idleTimeoutSecs: 30,
				},
			})

			if (result.sshHostKey) {
				sshHostKey = result.sshHostKey
				sshHostKeyRetry = () => handleTestConnection(draft)
				return
			}

			if (result.ok) {
				const version = result.serverVersion ? ` · ${result.serverVersion}` : ''
				reportTestResult(
					true,
					`Connection established to ${result.engine} (${result.latencyMs}ms)${version}. ${result.message}`,
					`Connected to ${result.engine} (${result.latencyMs}ms)`,
				)
			} else {
				reportTestResult(
					false,
					`Connection failed: ${prettifyConnectError(result.message)}`,
				)
			}
		} catch (err) {
			const prompt = parseSshHostKey(err)
			if (prompt) {
				sshHostKey = prompt
				sshHostKeyRetry = () => handleTestConnection(draft)
				return
			}
			reportTestResult(
				false,
				`Connection failed: ${prettifyConnectError(invokeErrorMessage(err))}`,
			)
		} finally {
			testing = false
		}
	}

	function cancelForm() {
		// Leave edit → back to that connection's detail.
		if (editing && selectedId) {
			editing = false
			creating = false
			testResult = null
			return
		}
		// Cancel create → reset the form.
		enterCreateMode()
	}

	/** Errors propagate to NewConnectionPanel, which shows them inline. */
	async function saveConnection(draft: ConnectionFormDraft) {
		saving = true
		try {
			const isUpdate = editing && !!selectedId
			const input = saveInputFromDraft(draft, {
				id: isUpdate ? selectedId! : undefined,
				existingPasswordSet: selected?.passwordSet,
			})

			const saved = await connectionProfiles.save(input)

			// The detail view of the saved profile is the confirmation.
			afterProfilesChange(saved.id)
		} finally {
			saving = false
		}
	}

	let deleting = $state(false)
	/** In-app confirm — `window.confirm` often opens *behind* the Tauri window on macOS. */
	let pendingDelete = $state<{ id: string; name: string } | null>(null)

	function requestDeleteConnection(id: string) {
		if (deleting) return
		const profile = connections.find((c) => c.id === id)
		if (!profile) return
		pendingDelete = { id: profile.id, name: profile.name }
	}

	function requestDeleteSelected() {
		if (!selected || deleting) return
		requestDeleteConnection(selected.id)
	}

	async function duplicateConnection(id: string) {
		const profile = connections.find((c) => c.id === id)
		if (!profile) return
		try {
			saving = true
			const cfg = { ...(profile.pluginConfig ?? {}) }
			const saved = await connectionProfiles.save({
				id: null,
				name: `${profile.name}-copy`,
				engine: String(profile.engine),
				colorId: profile.colorId,
				environment: profile.environment,
				readOnly: profile.readOnly,
				notes: profile.notes ?? null,
				pluginConfig: cfg,
				copySecretFrom:
					profile.passwordSet || profile.sshPasswordSet
						? profile.id
						: null,
				sshConfig: profile.ssh,
				queryTimeoutSecs: profile.queryTimeoutSecs ?? null,
				savePassword: profile.savePassword !== false,
			})
			afterProfilesChange(saved.id)
		} catch (err) {
			showToast('danger', err instanceof Error ? err.message : String(err))
		} finally {
			saving = false
		}
	}

	async function copyConnectionUrl(id: string) {
		const profile = connections.find((c) => c.id === id)
		if (!profile) return
		if (!isTauri()) {
			showToast(
				'warning',
				'Copy connection URL needs the Mystique desktop app.',
			)
			return
		}
		try {
			const formatted = await formatEngineConnectionUrl({
				engine: String(profile.engine),
				pluginConfig: (profile.pluginConfig ?? {}) as Record<string, unknown>,
			})
			const url = formatted.url?.trim()
			if (!url) {
				showToast('warning', 'No connection URL available for this profile.')
				return
			}
			const result = await copyTextToClipboard(url)
			if (result.ok) {
				showToast('success', 'Connection URL copied.')
			} else if (!result.cancelled) {
				showToast('danger', result.error || 'Could not copy connection URL.')
			}
		} catch (err) {
			showToast('danger', err instanceof Error ? err.message : String(err))
		}
	}

	function handleConnectionAction(action: ConnectionMenuAction, id: string) {
		if (action === 'view') {
			selectConnection(id)
			return
		}
		if (action === 'connect') {
			void connectById(id)
			return
		}
		if (action === 'edit') {
			editConnection(id)
			return
		}
		if (action === 'duplicate') {
			void duplicateConnection(id)
			return
		}
		if (action === 'delete') {
			requestDeleteConnection(id)
			return
		}
		if (action === 'copy-url') {
			void copyConnectionUrl(id)
			return
		}
		if (action === 'favorite') {
			void toggleFavorite(id)
		}
	}

	async function toggleFavorite(id: string) {
		try {
			await connectionProfiles.toggleFavorite(id)
		} catch (err) {
			showToast('danger', err instanceof Error ? err.message : String(err))
		}
	}

	function cancelPendingDelete() {
		if (deleting) return
		pendingDelete = null
	}

	function formatInvokeError(err: unknown): string {
		if (err instanceof Error) return err.message
		if (typeof err === 'string') return err
		if (err && typeof err === 'object') {
			const o = err as Record<string, unknown>
			if (typeof o.message === 'string') return o.message
			try {
				return JSON.stringify(err)
			} catch {
				/* fall through */
			}
		}
		return String(err)
	}

	async function confirmDeleteSelected() {
		if (!pendingDelete) return
		const { id } = pendingDelete
		deleting = true
		try {
			await connectionProfiles.remove(id)
			afterProfilesChange()
			// refreshList clears invalid selection → create mode.
			pendingDelete = null
		} catch (err) {
			showToast('danger', formatInvokeError(err))
		} finally {
			deleting = false
		}
	}

	async function duplicateSelected() {
		if (!selected) return
		await duplicateConnection(selected.id)
	}

	async function openWorkspace(
		profile: ConnectionProfile,
		engineSessionId?: string | null,
		capabilities?: import('#core/contracts').EngineCapabilities | null,
	) {
		workspaceSession.connect(
			{ ...profile, status: 'connected' },
			engineSessionId,
			capabilities,
		)
		await goto(`/workspace?c=${encodeURIComponent(profile.id)}`)
	}

	async function connectSelected() {
		if (!selected) return
		await connectProfile(selected)
	}

	async function connectById(id: string) {
		const profile = connections.find((c) => c.id === id)
		if (!profile || testing) return
		selectConnection(id)
		await connectProfile(profile)
	}

	async function connectProfile(profile: ConnectionProfile) {
		// FE preview: open workspace with dummy tables (no live engine).
		if (!isTauri()) {
			const { [profile.id]: _drop, ...rest } = statusOverride
			statusOverride = rest
			showToast(
				'info',
				'Opening workspace with sample data. Use the desktop app for a live database.',
			)
			await openWorkspace(profile)
			return
		}

		connectingId = profile.id
		testing = true
		testResult = null
		try {
			const pluginConfig = pluginConfigForSession(profile)

			// Asks once for a password the profile does not save.
			const result = await testSessionWithPassword({
				engine: String(profile.engine),
				pluginConfig,
				profileId: profile.id,
				pool: { maxSize: 2, minIdle: 0, acquireTimeoutMs: 8000 },
			}, profile.name)

			if (result.sshHostKey) {
				sshHostKey = result.sshHostKey
				sshHostKeyRetry = () => connectProfile(profile)
				return
			}

			if (!result.ok) {
				statusOverride = { ...statusOverride, [profile.id]: 'error' }
				reportTestResult(
					false,
					`Could not establish connection: ${prettifyConnectError(result.message)}`,
				)
				return
			}

			const updated = await connectionProfiles.touchConnected(profile.id)
			const { [profile.id]: _ok, ...rest } = statusOverride
			statusOverride = rest

			const session = await ensureSessionWithPassword({
				engine: String(profile.engine),
				pluginConfig,
				profileId: profile.id,
				pool: { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 },
				sessionId: profile.id,
			}, profile.name)

			const version = result.serverVersion ? ` · ${result.serverVersion}` : ''
			testResult = {
				ok: true,
				message: `Connection established (${result.latencyMs}ms)${version}`,
			}
			await openWorkspace(
				{ ...updated, status: 'connected' },
				session.sessionId,
				session.capabilities,
			)
		} catch (err) {
			if (isConnectCancelled(err)) return
			const prompt = parseSshHostKey(err)
			if (prompt) {
				sshHostKey = prompt
				sshHostKeyRetry = () => connectProfile(profile)
				return
			}
			statusOverride = { ...statusOverride, [profile.id]: 'error' }
			showToast(
				'danger',
				prettifyConnectError(invokeErrorMessage(err)),
			)
		} finally {
			testing = false
			connectingId = null
		}
	}

	onMount(() => {
		void setAppTitle()
		let unsubMenu = () => {}
		let cancelled = false
		void listenNativeMenu({
			'home-new-connection': startCreate,
			'home-import-url': openImportUrl,
			'home-connect': () => void connectSelected(),
			'home-edit': startEdit,
			'home-duplicate': () => void duplicateSelected(),
			'home-copy-url': () => {
				if (selected) void copyConnectionUrl(selected.id)
			},
			'home-delete': requestDeleteSelected,
			'home-refresh': () => void refreshConnections(),
		}).then((u) => {
			if (cancelled) u()
			else unsubMenu = u
		})
		;(async () => {
			loading = true
			loadError = null
			try {
				await refreshList()
				enterCreateMode()
				if (!isTauri()) {
					loadError = null
				}
			} catch (err) {
				loadError = err instanceof Error ? err.message : String(err)
			} finally {
				loading = false
			}
		})()
		const onQuickConnect = (event: Event) => {
			const id = (event as CustomEvent<{ id?: string }>).detail?.id
			if (id) void connectById(id)
		}
		window.addEventListener('mystique:quick-open-connect', onQuickConnect)
		return () => {
			cancelled = true
			unsubMenu()
			window.removeEventListener('mystique:quick-open-connect', onQuickConnect)
		}
	})
</script>

<svelte:window onkeydown={onHomeKeydown} />

<div class="flex h-screen w-screen overflow-hidden bg-app">
	{#if appChrome.homeSidebarOpen}
		<ConnectionSidebar
			{connections}
			{selectedId}
			creating={creating || editing}
			{connectingId}
			onselect={selectConnection}
			onnew={startCreate}
			onconnect={connectById}
			onaction={handleConnectionAction}
			onrefresh={refreshConnections}
			refreshing={listRefreshing}
		/>
	{/if}

	<main class="relative h-full min-w-0 flex-1 overflow-hidden bg-app-wash">
		<div class="relative z-10 h-full overflow-auto">
			{#if loading}
				<div
					class="flex h-full flex-col items-center justify-center gap-3 p-8"
					aria-busy="true"
				>
					<Icon name="refresh" class="size-5 animate-spin text-fg-muted" />
					<EmptyState title="Loading connections…" />
				</div>
			{:else if loadError}
				<div class="flex h-full items-center justify-center p-8">
					<EmptyState
						title="Could not load connections"
						description={loadError}
					/>
				</div>
			{:else if editing && selected}
				<NewConnectionPanel
					initial={selected}
					{saving}
					{testing}
					{testResult}
					oncancel={cancelForm}
					onsave={saveConnection}
					ontest={handleTestConnection}
				/>
			{:else if selected}
				<ConnectionDetail
					connection={selected}
					{testing}
					onconnect={connectSelected}
					ontest={async () => {
						await handleTestConnection(draftFromProfile(selected))
					}}
					onduplicate={duplicateSelected}
					ondelete={requestDeleteSelected}
					onedit={startEdit}
				/>
			{:else}
				{#key createFormKey}
					<NewConnectionPanel
						initial={null}
						{saving}
						{testing}
						{testResult}
						onsave={saveConnection}
						ontest={handleTestConnection}
					/>
				{/key}
			{/if}
		</div>
		{#if !loading && !loadError}
			<HomeIllustration />
		{/if}
	</main>

	<Modal
		open={!!pendingDelete}
		onclose={cancelPendingDelete}
		title="Delete connection?"
		description={pendingDelete
			? `Delete “${pendingDelete.name}”? Saved queries for this connection will also be removed. This cannot be undone.`
			: undefined}
		dismissible={!deleting}
	>
		{#snippet footer()}
			<Button variant="ghost" onclick={cancelPendingDelete} disabled={deleting}>
				Cancel
			</Button>
			<Button
				variant="danger"
				onclick={confirmDeleteSelected}
				disabled={deleting}
			>
				{deleting ? 'Deleting…' : 'Delete'}
			</Button>
		{/snippet}
	</Modal>

	<SshHostKeyModal
		prompt={sshHostKey}
		busy={testing}
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
			} catch (err) {
				showToast('danger', err instanceof Error ? err.message : String(err))
			}
		}}
	/>
</div>
