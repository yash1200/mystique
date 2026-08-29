<script lang="ts">
	import { isTauri } from '$core/ipc'
	import { Button, Switch } from '$core/ui'
	import { onMount } from 'svelte'
	import type { McpStatus } from '../api'
	import {
		getMcpStatus,
		mcpClientSnippet,
		rotateMcpToken,
		setMcpEnabled,
	} from '../api'

	let mcp = $state<McpStatus>({ enabled: false, running: false })
	let busy = $state(false)
	let error = $state<string | null>(null)
	let copied = $state<string | null>(null)

	const snippet = $derived(mcpClientSnippet(mcp))
	const desktop = isTauri()

	async function load() {
		if (!desktop) return
		try {
			mcp = await getMcpStatus()
			error = null
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
		} catch (err) {
			error = err instanceof Error ? err.message : String(err)
		} finally {
			busy = false
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
	{#if mcp.running && mcp.url}
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
			<div class="px-3 py-3">
				<p class="m-0 mb-1.5 text-caption font-medium text-fg-secondary">
					Client config
				</p>
				<pre
					class="m-0 max-h-40 overflow-auto rounded-md border border-line bg-input p-2 font-mono text-caption text-fg">{snippet}</pre>
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
			</div>
		</div>
		<p class="mt-3 mb-0 text-caption text-fg-muted">
			Turning the app off stops the server.
		</p>
	{:else if mcp.enabled && !mcp.running}
		<p class="mt-2 mb-0 text-caption text-fg-muted">
			Enabled, but the server is not running yet.
		</p>
	{/if}
{/if}
{#if error}
	<p class="mt-2 mb-0 text-caption text-danger" role="alert">{error}</p>
{/if}
