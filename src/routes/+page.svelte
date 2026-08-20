<script lang="ts">
	import { goto } from '$app/navigation'
	import { ensureEngineSession, testEngineConnection } from '$lib/api/engines'
	import {
		deleteProfile,
		listProfiles,
		saveProfile,
		touchProfileConnected,
	} from '$lib/api/profiles'
	import { isTauri } from '$lib/api/tauri'
	import { Button, EmptyState, Modal, Toast } from '$lib/components/core'
	import ConnectionDetail from '$lib/components/home/ConnectionDetail.svelte'
	import ConnectionSidebar from '$lib/components/home/ConnectionSidebar.svelte'
	import NewConnectionPanel from '$lib/components/home/NewConnectionPanel.svelte'
	import { workspaceSession } from '$lib/stores/workspace-session.svelte'
	import type {
		ConnectionFormDraft,
		ConnectionProfile,
	} from '$lib/types/connection'
	import {
		draftFromProfile,
		pluginConfigForSession,
		pluginConfigFromDraft,
		saveInputFromDraft,
	} from '$lib/types/connection'
	import { appPrefs } from '$lib/stores/app-prefs.svelte'
	import { setAppTitle } from '$lib/utils/appTitle'
	import { onMount } from 'svelte'

	type ToastState = {
		kind: 'info' | 'success' | 'warning' | 'danger'
		message: string
	} | null

	let connections = $state<ConnectionProfile[]>([])
	let selectedId = $state<string | null>(null)
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
	let loadError = $state<string | null>(null)
	let toast = $state<ToastState>(null)
	let toastTimer: ReturnType<typeof setTimeout> | null = null

	const selected = $derived(
		connections.find((c) => c.id === selectedId) ?? null,
	)

	function showToast(kind: NonNullable<ToastState>['kind'], message: string) {
		toast = { kind, message }
		if (toastTimer) clearTimeout(toastTimer)
		toastTimer = setTimeout(() => {
			toast = null
		}, 3600)
	}

	/** Landing: create-connection form. Connections live in the sidebar. */
	function enterCreateMode() {
		selectedId = null
		creating = true
		editing = false
		testResult = null
		createFormKey += 1
	}

	async function refreshList(preferSelectId?: string | null) {
		const rows = await listProfiles()
		connections = rows.map((r) => ({
			...r,
			status: connections.find((c) => c.id === r.id)?.status ?? 'disconnected',
		}))
		if (preferSelectId && connections.some((c) => c.id === preferSelectId)) {
			// After save/duplicate: show the new/updated profile detail.
			selectedId = preferSelectId
			creating = false
			editing = false
		} else if (!selectedId || !connections.some((c) => c.id === selectedId)) {
			if (!creating && !editing) {
				enterCreateMode()
			}
		}
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
		appPrefs.setLastConnectionId(id)
	}

	function startCreate() {
		enterCreateMode()
	}

	function startEdit() {
		if (!selected) return
		editing = true
		creating = false
		testResult = null
	}

	/**
	 * Live connectivity check via backend engine plugins.
	 * Reports established / failed clearly (toast + form banner).
	 */
	async function handleTestConnection(draft: ConnectionFormDraft) {
		testResult = null

		if (!isTauri()) {
			const msg =
				'Backend not available. Run with `npm run tauri dev` to test a real database connection.'
			testResult = { ok: false, message: msg }
			showToast('warning', msg)
			return
		}

		testing = true
		try {
			const pluginConfig = pluginConfigFromDraft(draft)
			// Prefer URI password when embedded; otherwise send form password
			const password = draft.password.trim().length > 0 ? draft.password : null

			const result = await testEngineConnection({
				engine: String(draft.engine),
				pluginConfig,
				password,
				pool: {
					maxSize: 2,
					minIdle: 0,
					acquireTimeoutMs: 8000,
					idleTimeoutSecs: 30,
				},
			})

			if (result.ok) {
				const version = result.serverVersion ? ` · ${result.serverVersion}` : ''
				const msg = `Connection established to ${result.engine} (${result.latencyMs}ms)${version}. ${result.message}`
				testResult = { ok: true, message: msg }
				showToast(
					'success',
					`Connected to ${result.engine} (${result.latencyMs}ms)`,
				)
			} else {
				const msg = `Connection failed: ${result.message}`
				testResult = { ok: false, message: msg }
				showToast('danger', msg)
			}
		} catch (err) {
			// Tauri may throw a string, Error, or { message } object
			let raw: string
			if (err instanceof Error) {
				raw = err.message
			} else if (typeof err === 'string') {
				raw = err
			} else if (err && typeof err === 'object' && 'message' in err) {
				raw = String((err as { message: unknown }).message)
			} else {
				raw = String(err)
			}
			const msg = `Connection failed: ${raw}`
			testResult = { ok: false, message: msg }
			showToast('danger', msg)
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

	async function saveConnection(draft: ConnectionFormDraft) {
		saving = true
		try {
			const isUpdate = editing && !!selectedId
			const input = saveInputFromDraft(draft, {
				id: isUpdate ? selectedId! : undefined,
				existingPasswordSet: selected?.passwordSet,
			})

			// Password is persisted plaintext with the profile (local SQLite).
			const saved = await saveProfile(input)

			await refreshList(saved.id)
			showToast(
				'success',
				isUpdate
					? `Updated “${saved.name}”.`
					: isTauri()
						? `Saved “${saved.name}” to local database.`
						: `Saved “${saved.name}” in this browser (FE preview).`,
			)
		} catch (err) {
			const msg = err instanceof Error ? err.message : String(err)
			showToast('danger', msg)
			throw err
		} finally {
			saving = false
		}
	}

	let deleting = $state(false)
	/** In-app confirm — `window.confirm` often opens *behind* the Tauri window on macOS. */
	let pendingDelete = $state<{ id: string; name: string } | null>(null)

	function requestDeleteSelected() {
		if (!selected || deleting) return
		pendingDelete = { id: selected.id, name: selected.name }
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
		const { id, name } = pendingDelete
		deleting = true
		try {
			await deleteProfile(id)
			// Re-fetch from SQLite so UI matches disk (do not only filter local state).
			await refreshList()
			// refreshList clears invalid selection → create mode.
			pendingDelete = null
			showToast('info', `Deleted “${name}”.`)
		} catch (err) {
			showToast('danger', formatInvokeError(err))
		} finally {
			deleting = false
		}
	}

	async function duplicateSelected() {
		if (!selected) return
		try {
			saving = true
			const cfg = { ...(selected.pluginConfig ?? {}) }
			const saved = await saveProfile({
				id: null,
				name: `${selected.name}-copy`,
				engine: String(selected.engine),
				colorId: selected.colorId,
				environment: selected.environment,
				readOnly: selected.readOnly,
				notes: selected.notes ?? null,
				pluginConfig: cfg,
				password: selected.password ?? null,
			})
			await refreshList(saved.id)
			showToast('success', `Duplicated as “${saved.name}”.`)
		} catch (err) {
			showToast('danger', err instanceof Error ? err.message : String(err))
		} finally {
			saving = false
		}
	}

	async function openWorkspace(
		profile: ConnectionProfile,
		engineSessionId?: string | null,
		capabilities?: import('$lib/types/engineCaps').EngineCapabilities | null,
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
			connections = connections.map((c) =>
				c.id === profile.id ? { ...c, status: 'connected' as const } : c,
			)
			showToast(
				'info',
				'Opening workspace with sample data (FE preview — run `npm run tauri dev` for live DB).',
			)
			await openWorkspace(profile)
			return
		}

		connectingId = profile.id
		testing = true
		testResult = null
		try {
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
				connections = connections.map((c) =>
					c.id === profile.id ? { ...c, status: 'error' as const } : c,
				)
				const msg = `Could not establish connection: ${result.message}`
				testResult = { ok: false, message: msg }
				showToast('danger', msg)
				return
			}

			const updated = await touchProfileConnected(profile.id)
			connections = connections.map((c) =>
				c.id === updated.id ? { ...updated, status: 'connected' as const } : c,
			)

			const session = await ensureEngineSession({
				engine: String(profile.engine),
				pluginConfig,
				password,
				pool: { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 },
				sessionId: profile.id,
			})

			const version = result.serverVersion ? ` · ${result.serverVersion}` : ''
			showToast(
				'success',
				`Connection established to ${result.engine} (${result.latencyMs}ms)${version}`,
			)
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
			connections = connections.map((c) =>
				c.id === profile.id ? { ...c, status: 'error' as const } : c,
			)
			showToast('danger', err instanceof Error ? err.message : String(err))
		} finally {
			testing = false
			connectingId = null
		}
	}

	onMount(() => {
		void setAppTitle()
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
				showToast('danger', loadError)
			} finally {
				loading = false
			}
		})()
	})
</script>

<div class="flex h-screen w-screen overflow-hidden bg-app">
	<ConnectionSidebar
		{connections}
		{selectedId}
		creating={creating || editing}
		{connectingId}
		onselect={selectConnection}
		onnew={startCreate}
		onconnect={connectById}
		onrefresh={refreshConnections}
		refreshing={listRefreshing}
	/>

	<main class="h-full min-w-0 flex-1 overflow-auto bg-app-wash">
		{#if loading}
			<div class="flex h-full items-center justify-center p-8">
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

	{#if toast}
		{#key toast.kind + toast.message}
			<Toast message={toast.message} tone={toast.kind} />
		{/key}
	{/if}
</div>
