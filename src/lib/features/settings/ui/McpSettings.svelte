<script lang="ts">
	import { isTauri } from '$core/ipc'
	import { Button, Switch } from '$core/ui'
	import { onMount } from 'svelte'
	import type { McpHarness, McpStatus } from '../api'
	import {
		getMcpStatus,
		installMcpHarness,
		listMcpHarnesses,
		mcpClientSnippet,
		rotateMcpToken,
		setMcpEnabled,
	} from '../api'
	import McpHarnesses from './McpHarnesses.svelte'

	let mcp = $state<McpStatus>({ enabled: false, running: false })
	let harnesses = $state<McpHarness[]>([])
	let busy = $state(false)
	let busyId = $state<string | null>(null)
	let error = $state<string | null>(null)
	let copied = $state<string | null>(null)

	const snippet = $derived(mcpClientSnippet(mcp))
	const desktop = isTauri()
	const ready = $derived(Boolean(mcp.url && mcp.token))

	async function loadHarnesses() {
		if (!desktop) return
		try {
			harnesses = await listMcpHarnesses()
		} catch (e) {
			error = e instanceof Error ? e.message : String(e)
		}
	}

	async function load() {
		if (!desktop) return
		try {
			mcp = await getMcpStatus()
			error = null
			await loadHarnesses()
		} catch (e) {
			error = e instanceof Error ? e.message : String(e)
		}
	}

	async function onEnable(on: boolean) {
		if (!desktop) return
		busy = true
		error = null
		try {
			mcp = await setMcpEnabled(on)
			await loadHarnesses()
		} catch (err) {
			error = err instanceof Error ? err.message : String(err)
			await load()
		} finally {
			busy = false
		}
	}

	async function onRotate() {
		if (!desktop) return
		busy = true
		error = null
		try {
			mcp = await rotateMcpToken()
			await loadHarnesses()
		} catch (err) {
			error = err instanceof Error ? err.message : String(err)
		} finally {
			busy = false
		}
	}

	async function onInstall(id: string) {
		if (!desktop) return
		busy = true
		busyId = id
		error = null
		try {
			const next = await installMcpHarness(id)
			harnesses = harnesses.map((h) => (h.id === next.id ? next : h))
		} catch (err) {
			error = err instanceof Error ? err.message : String(err)
			await loadHarnesses()
		} finally {
			busy = false
			busyId = null
		}
	}

	async function copy(text: string, key: string) {
		try {
			await navigator.clipboard.writeText(text)
			copied = key
			setTimeout(() => {
				if (copied === key) copied = null
			}, 1600)
		} catch {
			error = 'Could not copy. Select the text instead.'
		}
	}

	onMount(() => {
		void load()
	})
</script>

{#if !desktop}
	<p class="m-0 text-caption text-fg-muted">
		MCP is available in the desktop app.
	</p>
{:else}
	<Switch
		checked={mcp.enabled}
		disabled={busy}
		label="Enable MCP on localhost"
		onchange={onEnable}
	/>

	{#if ready}
		<div
			class="mt-4 flex flex-col divide-y divide-line overflow-hidden rounded-lg border border-line bg-panel"
		>
			<div class="px-3 py-3">
				<p class="m-0 mb-1.5 text-caption font-medium text-fg-secondary">URL</p>
				<div class="flex items-center gap-1.5">
					<code
						class="min-w-0 flex-1 truncate rounded-md border border-line bg-input px-2 py-1 font-mono text-caption text-fg"
						>{mcp.url}</code
					>
					<Button size="sm" onclick={() => void copy(mcp.url ?? '', 'url')}>
						{copied === 'url' ? 'Copied' : 'Copy'}
					</Button>
				</div>
			</div>
			<div class="px-3 py-3">
				<p class="m-0 mb-1.5 text-caption font-medium text-fg-secondary">
					Token
				</p>
				<div class="flex items-center gap-1.5">
					<code
						class="min-w-0 flex-1 truncate rounded-md border border-line bg-input px-2 py-1 font-mono text-caption text-fg"
						>{mcp.token}</code
					>
					<Button size="sm" onclick={() => void copy(mcp.token ?? '', 'token')}>
						{copied === 'token' ? 'Copied' : 'Copy'}
					</Button>
				</div>
			</div>
		</div>

		{#if harnesses.length > 0}
			<section class="mt-5">
				<p
					class="m-0 mb-1 text-label font-semibold tracking-wider text-fg-muted uppercase"
				>
					On this machine
				</p>
				<p class="mt-0 mb-2 text-caption text-fg-muted">
					Add Mystique to an AI tool already installed here.
				</p>
				<div class="overflow-hidden rounded-lg border border-line bg-panel">
					<McpHarnesses
						{harnesses}
						{busy}
						{busyId}
						oninstall={(id) => void onInstall(id)}
					/>
				</div>
			</section>
		{/if}

		<section class="mt-5">
			<p
				class="m-0 mb-1 text-label font-semibold tracking-wider text-fg-muted uppercase"
			>
				Manual config
			</p>
			<p class="mt-0 mb-2 text-caption text-fg-muted">
				{#if harnesses.length > 0}
					For Cursor and any other client that is not listed above.
				{:else}
					Paste this into Cursor or any other MCP client.
				{/if}
			</p>
			<pre
				class="m-0 max-h-40 overflow-auto rounded-lg border border-line bg-panel p-3 font-mono text-caption text-fg">{snippet}</pre>
			<div class="mt-2 flex flex-wrap gap-1.5">
				<Button size="sm" onclick={() => void copy(snippet, 'snippet')}>
					{copied === 'snippet' ? 'Copied' : 'Copy config'}
				</Button>
				<Button
					size="sm"
					variant="ghost"
					disabled={busy}
					onclick={() => void onRotate()}
				>
					New token
				</Button>
			</div>
		</section>

		<p class="mt-3 mb-0 text-caption text-fg-muted">
			{#if mcp.running}
				Turning the app off stops the server.
			{:else if mcp.enabled}
				Enabled, but the server is not running yet.
			{:else}
				Clients can be added now. They connect only while the server is on.
			{/if}
		</p>
	{/if}
{/if}
{#if error}
	<p class="mt-2 mb-0 text-caption text-danger" role="alert">{error}</p>
{/if}
