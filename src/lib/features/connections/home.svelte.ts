/**
 * The home screen's state and actions: which view is up (the create form, a
 * connection's detail, its edit form), the saved list with live status, and
 * the test / connect / save / delete flows. `ConnectionsPage` creates one and
 * hands it to its children through `setHome`.
 *
 * Not exported from the feature index: it composes the workspace session,
 * which itself reads this feature.
 */
import { commands } from '#core/commands'
import type {
	ConnectionFormDraft,
	ConnectionProfile,
	ConnectionStatus,
	TestResult,
} from '#core/contracts'
import { errorMessage, testEngineConnection } from '#core/ipc'
import { setAppTitle } from '#core/util'
import { showToast } from '#features/toast'
import { workspaceSession } from '#features/workspace/session'
import { OPEN_URL_IMPORT } from '#engines'
import { cmd } from '#lib/app'
import { goto } from '$app/navigation'
import { createContext } from 'svelte'
import { SvelteSet } from 'svelte/reactivity'
import { copyConnectionUrl } from './facts'
import {
	pluginConfigForSession,
	pluginConfigFromDraft,
	saveInputFromDraft,
} from './model'
import {
	ensureSessionWithPassword,
	testSessionWithPassword,
} from './password-prompt.svelte'
import { isConnectCancelled } from './password-prompt'
import { connectionProfiles } from './profiles.svelte'
import { sshHostKeys } from './ssh-host-key.svelte'
import {
	connectErrorMessage,
	prettifyConnectError,
	sshTunnelFromDraft,
} from './ssh'

export type HomeView =
	| { kind: 'create'; key: number }
	| { kind: 'detail'; id: string }
	| { kind: 'edit'; id: string }

export type HomeConnection = ConnectionProfile & { status: ConnectionStatus }

/** The last test or connect, shown by the form or detail that ran it. */
export type Health = {
	/** The profile it ran for; null for the unsaved create form. */
	id: string | null
	ok: boolean
	message: string
	latencyMs?: number
	version?: string | null
}

export type ConnectionMenuAction =
	| 'view'
	| 'connect'
	| 'edit'
	| 'duplicate'
	| 'delete'
	| 'copy-url'
	| 'favorite'

type Load =
	| { state: 'loading' }
	| { state: 'ready' }
	| { state: 'error'; message: string }

const TEST_POOL = { maxSize: 2, minIdle: 0, acquireTimeoutMs: 8000 }
const DRAFT_TEST_POOL = { ...TEST_POOL, idleTimeoutSecs: 30 }
const SESSION_POOL = { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 }

/** How long the mascot stays happy after a passing test. */
const CHEER_MS = 2200

export class HomeController {
	view = $state<HomeView>({ kind: 'create', key: 0 })
	load = $state<Load>({ state: 'loading' })
	refreshing = $state(false)
	saving = $state(false)
	/** The test or connect in flight; `id` is null for the create form. */
	busy = $state<{ kind: 'test' | 'connect'; id: string | null } | null>(null)
	health = $state<Health | null>(null)
	pendingDelete = $state<{ id: string; name: string } | null>(null)
	deleting = $state(false)
	/** A test just passed: the mascot wags. */
	happy = $state(false)

	/** Profiles whose last connect failed, until one succeeds. */
	#failed = new SvelteSet<string>()
	#createKey = 0
	#cheerTimer: ReturnType<typeof setTimeout> | undefined

	connections = $derived<HomeConnection[]>(
		connectionProfiles.items.map((profile) => ({
			...profile,
			status: this.#failed.has(profile.id)
				? 'error'
				: workspaceSession.sessions.some((s) => s.profile.id === profile.id)
					? 'connected'
					: 'disconnected',
		})),
	)

	selectedId = $derived(this.view.kind === 'create' ? null : this.view.id)
	selected = $derived(
		this.connections.find((c) => c.id === this.selectedId) ?? null,
	)
	/** The edit form is up for a profile that still exists. */
	editing = $derived(this.view.kind === 'edit' && this.selected != null)
	connectingId = $derived(
		this.busy?.kind === 'connect' ? this.busy.id : null,
	)

	find = (id: string | null | undefined): HomeConnection | undefined =>
		id ? this.connections.find((c) => c.id === id) : undefined

	// —— Views ——

	create = () => {
		this.#createKey += 1
		this.view = { kind: 'create', key: this.#createKey }
		this.health = null
	}

	select = (id: string) => {
		this.view = { kind: 'detail', id }
		this.health = null
	}

	edit = (id = this.selectedId) => {
		if (!this.find(id)) return
		this.view = { kind: 'edit', id: id! }
		this.health = null
	}

	/** Leave the form: an edit goes back to its detail, a create starts over. */
	cancelForm = () => {
		if (this.view.kind === 'edit') this.select(this.view.id)
		else this.create()
	}

	/** Esc: the edit form goes back to its detail, a detail to the create form. */
	canGoBack = (): boolean =>
		this.view.kind !== 'create' &&
		!this.pendingDelete &&
		!document.querySelector('[role="dialog"], [role="menu"]')

	back = () => {
		if (this.view.kind === 'edit') this.cancelForm()
		else this.create()
	}

	/** After the list changes: show `preferId`, or the create form when the one in view is gone. */
	#settle(preferId?: string) {
		if (this.find(preferId)) {
			this.view = { kind: 'detail', id: preferId! }
		} else if (this.view.kind !== 'create' && !this.find(this.view.id)) {
			this.create()
		}
	}

	// —— List ——

	refresh = async () => {
		if (this.refreshing) return
		this.refreshing = true
		try {
			await connectionProfiles.refresh()
			this.#settle()
		} catch (err) {
			showToast('danger', errorMessage(err))
		} finally {
			this.refreshing = false
		}
	}

	toggleFavorite = async (id: string) => {
		try {
			await connectionProfiles.toggleFavorite(id)
		} catch (err) {
			showToast('danger', errorMessage(err))
		}
	}

	duplicate = async (id = this.selectedId) => {
		if (!id || this.saving) return
		this.saving = true
		try {
			const copy = await connectionProfiles.duplicate(id)
			this.#settle(copy.id)
		} catch (err) {
			showToast('danger', errorMessage(err))
		} finally {
			this.saving = false
		}
	}

	copyUrl = (id = this.selectedId) => {
		const profile = this.find(id)
		if (profile) void copyConnectionUrl(profile)
	}

	/** Open the URL import on the create form. */
	importUrl = () => {
		if (this.view.kind !== 'create') this.create()
		setTimeout(() => commands.run(OPEN_URL_IMPORT), 0)
	}

	/** The connection context menu. */
	act = (action: ConnectionMenuAction, id: string) => {
		const handlers: Record<ConnectionMenuAction, (id: string) => unknown> = {
			view: this.select,
			connect: this.connect,
			edit: this.edit,
			duplicate: this.duplicate,
			delete: this.requestDelete,
			'copy-url': this.copyUrl,
			favorite: this.toggleFavorite,
		}
		void handlers[action](id)
	}

	// —— Save / delete ——

	/** Errors reach the form, which shows them inline. */
	save = async (draft: ConnectionFormDraft) => {
		const id = this.view.kind === 'edit' ? this.view.id : undefined
		this.saving = true
		try {
			const saved = await connectionProfiles.save(
				saveInputFromDraft(draft, {
					id,
					existingPasswordSet: this.find(id)?.passwordSet,
				}),
			)
			// The detail of the saved profile is the confirmation.
			this.#settle(saved.id)
		} finally {
			this.saving = false
		}
	}

	/** Asks in-app: `window.confirm` can open behind the Tauri window on macOS. */
	requestDelete = (id = this.selectedId) => {
		const profile = this.find(id)
		if (profile && !this.deleting) {
			this.pendingDelete = { id: profile.id, name: profile.name }
		}
	}

	cancelDelete = () => {
		if (!this.deleting) this.pendingDelete = null
	}

	confirmDelete = async () => {
		const target = this.pendingDelete
		if (!target) return
		this.deleting = true
		try {
			await connectionProfiles.remove(target.id)
			this.#settle()
			this.pendingDelete = null
		} catch (err) {
			showToast('danger', errorMessage(err))
		} finally {
			this.deleting = false
		}
	}

	// —— Test / connect ——

	/** The form's draft, against the engine plugin, before it is saved. */
	testDraft = async (draft: ConnectionFormDraft) => {
		const id = this.view.kind === 'edit' ? this.view.id : null
		this.health = null
		this.busy = { kind: 'test', id }
		try {
			const tlsPass = (draft.sslClientKeyPassphrase ?? '').trim()
			const result = await testEngineConnection({
				engine: String(draft.engine),
				pluginConfig: pluginConfigFromDraft(draft),
				password: draft.password.trim() || null,
				profileId: id ?? undefined,
				ssh: sshTunnelFromDraft(draft),
				useSavedSsh: false,
				tlsKeyPassphrase: tlsPass || null,
				pool: DRAFT_TEST_POOL,
			})
			if (sshHostKeys.offer(result, () => this.testDraft(draft))) return
			this.#report(id, result)
		} catch (err) {
			if (sshHostKeys.offer(err, () => this.testDraft(draft))) return
			this.health = {
				id,
				ok: false,
				message: `Connection failed: ${connectErrorMessage(err)}`,
			}
		} finally {
			this.busy = null
		}
	}

	/** A saved profile with its saved secrets, asking for a password it does not keep. */
	testSaved = async (id = this.selectedId) => {
		const profile = this.find(id)
		if (!profile || this.busy) return
		this.health = null
		this.busy = { kind: 'test', id: profile.id }
		try {
			const result = await testSessionWithPassword(
				{
					engine: String(profile.engine),
					pluginConfig: pluginConfigForSession(profile),
					profileId: profile.id,
					pool: TEST_POOL,
				},
				profile.name,
			)
			if (sshHostKeys.offer(result, () => this.testSaved(profile.id))) return
			this.#report(profile.id, result)
		} catch (err) {
			if (isConnectCancelled(err)) return
			if (sshHostKeys.offer(err, () => this.testSaved(profile.id))) return
			this.health = {
				id: profile.id,
				ok: false,
				message: `Connection failed: ${connectErrorMessage(err)}`,
			}
		} finally {
			this.busy = null
		}
	}

	/** Test, open the session, and go to the workspace. */
	connect = async (id = this.selectedId) => {
		const profile = this.find(id)
		if (!profile || this.busy) return
		if (this.selectedId !== profile.id) this.select(profile.id)
		this.health = null
		this.busy = { kind: 'connect', id: profile.id }
		try {
			const pluginConfig = pluginConfigForSession(profile)
			// Asks once for a password the profile does not save.
			const result = await testSessionWithPassword(
				{
					engine: String(profile.engine),
					pluginConfig,
					profileId: profile.id,
					pool: TEST_POOL,
				},
				profile.name,
			)
			if (sshHostKeys.offer(result, () => this.connect(profile.id))) return
			if (!result.ok) {
				this.#failed.add(profile.id)
				this.health = {
					id: profile.id,
					ok: false,
					message: `Could not establish connection: ${prettifyConnectError(result.message)}`,
				}
				return
			}

			const updated = await connectionProfiles.touchConnected(profile.id)
			this.#failed.delete(profile.id)
			const session = await ensureSessionWithPassword(
				{
					engine: String(profile.engine),
					pluginConfig,
					profileId: profile.id,
					pool: SESSION_POOL,
					sessionId: profile.id,
				},
				profile.name,
			)
			this.#report(profile.id, result)
			workspaceSession.connect(
				{ ...updated, status: 'connected' },
				session.sessionId,
				session.capabilities,
			)
			await goto(`/workspace?c=${encodeURIComponent(profile.id)}`)
		} catch (err) {
			if (isConnectCancelled(err)) return
			if (sshHostKeys.offer(err, () => this.connect(profile.id))) return
			this.#failed.add(profile.id)
			this.health = { id: profile.id, ok: false, message: connectErrorMessage(err) }
		} finally {
			this.busy = null
		}
	}

	#report(id: string | null, result: TestResult) {
		if (!result.ok) {
			this.health = {
				id,
				ok: false,
				message: `Connection failed: ${prettifyConnectError(result.message)}`,
			}
			return
		}
		const version = result.serverVersion ? ` · ${result.serverVersion}` : ''
		this.health = {
			id,
			ok: true,
			latencyMs: result.latencyMs,
			version: result.serverVersion,
			message: `Connection established to ${result.engine} (${result.latencyMs}ms)${version}. ${result.message}`.trim(),
		}
		this.happy = true
		clearTimeout(this.#cheerTimer)
		this.#cheerTimer = setTimeout(() => (this.happy = false), CHEER_MS)
	}

	// —— Lifecycle ——

	/** Register the home commands and load the list. Returns the cleanup. */
	mount = () => {
		void setAppTitle()
		const unregister = commands.register([
			{ ...cmd.homeBack, when: this.canGoBack, run: this.back },
			{ ...cmd.homeNewConnection, run: this.create },
			{ ...cmd.homeImportUrl, run: this.importUrl },
			{ ...cmd.homeConnect, run: () => void this.connect() },
			{ ...cmd.homeEdit, run: () => this.edit() },
			{ ...cmd.homeDuplicate, run: () => void this.duplicate() },
			{ ...cmd.homeCopyUrl, run: () => this.copyUrl() },
			{ ...cmd.homeDelete, run: () => this.requestDelete() },
			{ ...cmd.homeRefresh, run: () => void this.refresh() },
			{
				...cmd.openConnectionById,
				run: ({ args }) => {
					if (args?.id) void this.connect(args.id)
				},
			},
		])
		void this.#load()
		return () => {
			unregister()
			clearTimeout(this.#cheerTimer)
		}
	}

	async #load() {
		this.load = { state: 'loading' }
		try {
			await connectionProfiles.refresh()
			this.create()
			this.load = { state: 'ready' }
		} catch (err) {
			this.load = { state: 'error', message: errorMessage(err) }
		}
	}
}

export const [getHome, setHome] = createContext<HomeController>()
