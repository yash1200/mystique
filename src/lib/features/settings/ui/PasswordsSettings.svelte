<script lang="ts">
	import { isTauri } from '#core/ipc'
	import { Button, ControlGroup, ControlRow } from '#core/ui'
	import { onMount } from 'svelte'
	import {
		cleanUpKeychain,
		getVaultStatus,
		retryVaultUnlock,
		type KeychainCleanup,
		type VaultStatus,
	} from '../api'

	const desktop = isTauri()
	let status = $state<VaultStatus | null>(null)
	let cleanup = $state<KeychainCleanup | null>(null)
	let busy = $state(false)
	let error = $state<string | null>(null)

	const keychain = $derived(status?.source === 'keychain')

	function describe(s: VaultStatus | null): string {
		if (!s) return ''
		if (s.source !== 'keychain') {
			return 'This build keeps the key in a file in the app data folder and never uses the keychain.'
		}
		if (s.state === 'unlocked') return 'Unlocked for this session.'
		if (s.state === 'denied') {
			return s.message
				? `Keychain access was refused: ${s.message}`
				: 'Keychain access was refused.'
		}
		return 'Unlocks the first time a saved password is needed.'
	}

	function describeCleanup(c: KeychainCleanup): string {
		if (!c.supported) return 'This build does not use the keychain.'
		if (c.deleted === 0 && c.skipped === 0) return 'Nothing to clean up.'
		const deleted = `Deleted ${c.deleted} old ${c.deleted === 1 ? 'item' : 'items'}.`
		if (c.skipped === 0) return deleted
		return `${deleted} Skipped ${c.skipped} that would have needed a keychain prompt.`
	}

	async function run(work: () => Promise<void>) {
		busy = true
		error = null
		try {
			await work()
		} catch (e) {
			error = e instanceof Error ? e.message : String(e)
		} finally {
			busy = false
		}
	}

	function retry() {
		void run(async () => {
			status = await retryVaultUnlock()
		})
	}

	function cleanUp() {
		void run(async () => {
			cleanup = await cleanUpKeychain()
		})
	}

	onMount(() => {
		if (!desktop) return
		void run(async () => {
			status = await getVaultStatus()
		})
	})
</script>

{#if !desktop}
	<p class="m-0 text-caption text-fg-muted">
		Saved passwords are available in the desktop app.
	</p>
{:else}
	<div class="flex max-w-panel-md flex-col">
		<ControlGroup labelledby="settings-pane-title">
			<ControlRow label="Keychain" description={describe(status)}>
				<Button
					size="sm"
					disabled={busy || !keychain || status?.state === 'unlocked'}
					onclick={retry}
				>
					Retry keychain
				</Button>
			</ControlRow>
			<ControlRow
				label="Old keychain items"
				description="Earlier versions kept one keychain item per password. Delete the ones no connection uses. Never asks for your password."
			>
				<Button size="sm" disabled={busy || !keychain} onclick={cleanUp}>
					Clean up
				</Button>
			</ControlRow>
		</ControlGroup>
		{#if cleanup}
			<p class="mt-2 mb-0 text-caption text-fg-muted" role="status">
				{describeCleanup(cleanup)}
			</p>
		{/if}
		{#if error}
			<p class="mt-2 mb-0 text-caption text-danger" role="alert">{error}</p>
		{/if}
		<p class="mt-3 mb-0 max-w-prose-ui text-caption leading-relaxed text-fg-muted">
			Saved passwords, SSH and TLS secrets, AI keys and the MCP token are
			encrypted in the app database with one key. Release builds keep that key
			in the macOS keychain, so macOS asks at most once per launch.
		</p>
	</div>
{/if}
