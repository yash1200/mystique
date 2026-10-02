<script lang="ts">
	import { isTauri } from '#core/ipc'
	import {
		Button,
		Checkbox,
		ControlGroup,
		ControlRow,
		Segmented,
		Switch,
		type SegmentedOption,
	} from '#core/ui'
	import { engineLabel } from '#engines'
	import { onMount } from 'svelte'
	import type { McpAccess, McpHarness, McpStatus, McpWritePolicy } from '../api'
	import {
		getMcpAccess,
		getMcpStatus,
		installMcpHarness,
		listMcpHarnesses,
		mcpClientSnippet,
		rotateMcpToken,
		setMcpAccess,
		setMcpEnabled,
	} from '../api'
	import McpHarnesses from './McpHarnesses.svelte'

	let mcp = $state<McpStatus>({ enabled: false, running: false })
	let access = $state<McpAccess>({
		policy: 'read-only',
		allowlist: [],
		connections: [],
	})
	let harnesses = $state<McpHarness[]>([])
	let busy = $state(false)
	let busyId = $state<string | null>(null)
	let error = $state<string | null>(null)
	let copied = $state<string | null>(null)

	const policyOptions: SegmentedOption[] = [
		{ value: 'read-only', label: 'Read-only' },
		{ value: 'safe_write', label: 'Safe write' },
		{ value: 'high_risk_write', label: 'High-risk' },
	]

	function isPolicy(v: string): v is McpWritePolicy {
		return v === 'read-only' || v === 'safe_write' || v === 'high_risk_write'
	}

	const policyHint = $derived(
		access.policy === 'safe_write'
			? 'INSERT and UPDATE on allowlisted connections. DELETE, DROP, and DDL stay blocked.'
			: access.policy === 'high_risk_write'
				? 'DDL and DROP allowed on allowlisted connections. Production still needs the in-app write unlock.'
				: 'SELECT and find only. Writes are blocked even if the connection is not read-only.',
	)

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

	async function loadAccess() {
		if (!desktop) return
		access = await getMcpAccess()
	}

	async function load() {
		if (!desktop) return
		try {
			mcp = await getMcpStatus()
			error = null
			await loadAccess()
			await loadHarnesses()
		} catch (e) {
			error = e instanceof Error ? e.message : String(e)
		}
	}

	async function persistAccess(policy: McpWritePolicy, allowlist: string[]) {
		if (!desktop) return
		busy = true
		error = null
		try {
			access = await setMcpAccess(policy, allowlist)
		} catch (err) {
			error = err instanceof Error ? err.message : String(err)
			await loadAccess()
		} finally {
			busy = false
		}
	}

	function onPolicy(v: string) {
		if (!isPolicy(v) || v === access.policy) return
		void persistAccess(v, access.allowlist)
	}

	function onToggleConn(id: string, on: boolean) {
		const next = on
			? [...new Set([...access.allowlist, id])]
			: access.allowlist.filter((x) => x !== id)
		void persistAccess(access.policy, next)
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

	<section class="mt-5">
		<p
			class="m-0 mb-1 text-label font-semibold tracking-wider text-fg-muted uppercase"
		>
			Access
		</p>
		<p class="mt-0 mb-2 text-caption text-fg-muted">
			Empty allowlist means MCP cannot see any database. Reconnect
			clients after changing policy so they see the updated tools.
		</p>
		<ControlGroup>
			<ControlRow label="Write policy" description={policyHint}>
				<Segmented
					label="Write policy"
					value={access.policy}
					options={policyOptions}
					disabled={busy}
					onchange={onPolicy}
				/>
			</ControlRow>
		</ControlGroup>
		{#if access.connections.length === 0}
			<p class="mt-3 mb-0 text-caption text-fg-muted">
				Save a connection first, then allow it here.
			</p>
		{:else}
			<div
				class="mt-3 overflow-hidden rounded-lg border border-line bg-panel"
			>
				<ul class="m-0 flex list-none flex-col divide-y divide-line p-0">
					{#each access.connections as c (c.id)}
						<li class="px-3 py-2.5">
							<Checkbox
								checked={access.allowlist.includes(c.id)}
								disabled={busy}
								onchange={(e) =>
									onToggleConn(
										c.id,
										(e.currentTarget as HTMLInputElement).checked,
									)}
							>
								<span class="flex min-w-0 flex-col">
									<span class="truncate text-label font-medium text-fg"
										>{c.name}</span
									>
									<span class="text-caption text-fg-muted"
										>{engineLabel(c.engine)}</span
									>
								</span>
							</Checkbox>
						</li>
					{/each}
				</ul>
			</div>
		{/if}
	</section>

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
