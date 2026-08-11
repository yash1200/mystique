<script lang="ts">
	import { goto } from '$app/navigation'
	import { page } from '$app/state'
	import { openEngineSession } from '$lib/api/engines'
	import { listProfiles } from '$lib/api/profiles'
	import { isTauri } from '$lib/api/tauri'
	import WorkspaceShell from '$lib/components/workspace/WorkspaceShell.svelte'
	import { workspaceSession } from '$lib/stores/workspace-session.svelte'
	import type { ConnectionProfile } from '$lib/types/connection'
	import {
		draftFromProfile,
		pluginConfigForSession,
	} from '$lib/types/connection'
	import { writeLastConnectionId } from '$lib/utils/appPrefs'
	import { onMount } from 'svelte'

	let ready = $state(false)
	let bootError = $state<string | null>(null)

	/**
	 * Ensure the connection from ?c= is in the session store.
	 * If the user landed without connecting, load profile and open a mock session.
	 */
	async function ensureSessionFromQuery() {
		const connectionId = page.url.searchParams.get('c')
		if (!connectionId) {
			if (workspaceSession.sessions.length === 0) {
				await goto('/')
				return
			}
			return
		}

		const existing = workspaceSession.sessions.find(
			(s) => s.profile.id === connectionId,
		)
		if (existing) {
			workspaceSession.setActiveConnection(connectionId)
			writeLastConnectionId(connectionId)
			// Ensure live engine session if missing (e.g. soft reload).
			if (isTauri() && !existing.engineSessionId) {
				try {
					const draft = draftFromProfile(existing.profile)
					const pluginConfig = pluginConfigForSession(existing.profile)
					const password =
						draft.password.trim().length > 0
							? draft.password
							: existing.profile.password?.trim() || null
					const session = await openEngineSession({
						engine: String(existing.profile.engine),
						pluginConfig,
						password,
						pool: { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 },
						sessionId: existing.profile.id,
					})
					workspaceSession.connect(
						existing.profile,
						session.sessionId,
						session.capabilities,
					)
				} catch {
					/* query will show "no session" */
				}
			}
			return
		}

		try {
			const rows = await listProfiles()
			const profile: ConnectionProfile | undefined = rows.find(
				(r) => r.id === connectionId,
			)
			if (!profile) {
				bootError = 'Connection not found.'
				return
			}
			let engineSessionId: string | null = null
			let capabilities = null
			if (isTauri()) {
				try {
					const draft = draftFromProfile(profile)
					const pluginConfig = pluginConfigForSession(profile)
					const password =
						draft.password.trim().length > 0
							? draft.password
							: profile.password?.trim() || null
					const session = await openEngineSession({
						engine: String(profile.engine),
						pluginConfig,
						password,
						pool: { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 },
						sessionId: profile.id,
					})
					engineSessionId = session.sessionId
					capabilities = session.capabilities
				} catch (e) {
					bootError =
						e instanceof Error
							? `Could not open engine session: ${e.message}`
							: String(e)
					return
				}
			}
			writeLastConnectionId(connectionId)
			workspaceSession.connect(
				{ ...profile, status: 'connected' },
				engineSessionId,
				capabilities,
			)
		} catch (e) {
			bootError = e instanceof Error ? e.message : String(e)
		}
	}

	onMount(() => {
		;(async () => {
			await ensureSessionFromQuery()
			if (workspaceSession.sessions.length === 0 && !bootError) {
				await goto('/')
				return
			}
			ready = true
		})()
	})
</script>

{#if bootError}
	<div class="flex h-screen items-center justify-center bg-app p-6">
		<div class="max-w-md text-center">
			<p class="text-body text-danger">{bootError}</p>
			<a class="mt-3 inline-block text-body text-accent" href="/"
				>Back to home</a
			>
		</div>
	</div>
{:else if ready}
	<WorkspaceShell />
{:else}
	<div
		class="flex h-screen items-center justify-center bg-app text-caption text-fg-muted"
	>
		Opening workspace…
	</div>
{/if}
