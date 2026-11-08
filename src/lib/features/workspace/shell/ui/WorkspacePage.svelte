<script lang="ts">
	import { goto } from '$app/navigation'
	import { page } from '$app/state'
	import type { ConnectionProfile } from '#core/contracts'
	import { isTauri } from '#core/ipc'
	import {
		connectionProfiles,
		ensureSessionWithPassword,
		isConnectCancelled,
		pluginConfigForSession,
	} from '#features/connections'
	import { onMount } from 'svelte'
	import { workspaceSession } from '../session.svelte'
	import WorkspaceShell from './WorkspaceShell.svelte'

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
			// Ensure live engine session if missing (e.g. soft reload).
			if (isTauri() && !existing.engineSessionId) {
				try {
					const pluginConfig = pluginConfigForSession(existing.profile)
					const session = await ensureSessionWithPassword(
						{
							engine: String(existing.profile.engine),
							pluginConfig,
							pool: { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 },
							sessionId: existing.profile.id,
							profileId: existing.profile.id,
						},
						existing.profile.name,
					)
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
			await connectionProfiles.ensureLoaded()
			let profile: ConnectionProfile | undefined =
				connectionProfiles.profile(connectionId)
			if (!profile) {
				await connectionProfiles.refresh()
				profile = connectionProfiles.profile(connectionId)
			}
			if (!profile) {
				bootError = 'Connection not found.'
				return
			}
			let engineSessionId: string | null = null
			let capabilities = null
			if (isTauri()) {
				try {
					const pluginConfig = pluginConfigForSession(profile)
					const session = await ensureSessionWithPassword(
						{
							engine: String(profile.engine),
							pluginConfig,
							pool: { maxSize: 5, minIdle: 0, acquireTimeoutMs: 10000 },
							sessionId: profile.id,
							profileId: profile.id,
						},
						profile.name,
					)
					engineSessionId = session.sessionId
					capabilities = session.capabilities
				} catch (e) {
					// Prompt closed: no error page, the boot falls back to home.
					if (isConnectCancelled(e)) return
					bootError =
						e instanceof Error
							? `Could not open engine session: ${e.message}`
							: String(e)
					return
				}
			}
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
