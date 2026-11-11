<script lang="ts">
	import { errorMessage } from '#core/ipc'
	import {
		Button,
		ControlGroup,
		ControlRow,
		Segmented,
		Switch,
		type SegmentedOption,
	} from '#core/ui'
	import { onMount } from 'svelte'
	import { ACCESS_LEVELS, isAccessLevel, levelHint, levelLabel } from '../access-levels'
	import type { AccessLevel, McpHarness, McpStatus, ToolAccess } from '../api'
	import {
		getMcpStatus,
		getToolAccess,
		installMcpHarness,
		listMcpHarnesses,
		mcpClientSnippet,
		rotateMcpToken,
		setMcpEnabled,
		setToolAccessAsk,
		setToolAccessDefault,
		setToolAccessLevel,
	} from '../api'
	import McpHarnesses from './McpHarnesses.svelte'
	import ToolAccessList from './ToolAccessList.svelte'

	let mcp = $state<McpStatus>({
		enabled: false,
		running: false,
		port: null,
		url: null,
		token: null,
	})
	let access = $state<ToolAccess>({
		mcpDefault: 'none',
		agentDefault: 'read',
		connections: [],
	})
	let harnesses = $state<McpHarness[]>([])
	let busy = $state(false)
	let busyId = $state<string | null>(null)
	let error = $state<string | null>(null)
	let copied = $state<string | null>(null)

	const levelOptions: SegmentedOption[] = ACCESS_LEVELS.map((l) => ({
		value: l,
		label: levelLabel(l),
	}))

	const defaultHint = $derived(
		`${levelHint(access.mcpDefault)} Applies to connections with no level of their own; it never allows writes on a production connection.`,
	)

	const snippet = $derived(mcpClientSnippet(mcp))
	const ready = $derived(Boolean(mcp.url && mcp.token))

	async function loadHarnesses() {
		try {
			harnesses = await listMcpHarnesses()
		} catch (e) {
			error = errorMessage(e)
		}
	}

	async function loadAccess() {
		access = await getToolAccess()
	}

	async function load() {
		try {
			mcp = await getMcpStatus()
			error = null
			await loadAccess()
			await loadHarnesses()
		} catch (e) {
			error = errorMessage(e)
		}
	}

	async function persistAccess(change: () => Promise<ToolAccess>) {
		busy = true
		error = null
		try {
			access = await change()
		} catch (err) {
			error = errorMessage(err)
			await loadAccess()
		} finally {
			busy = false
		}
	}

	function onDefault(v: string) {
		if (!isAccessLevel(v) || v === access.mcpDefault) return
		void persistAccess(() => setToolAccessDefault('mcp', v))
	}

	function onLevel(
		connectionId: string,
		database: string | null,
		level: AccessLevel | null,
	) {
		void persistAccess(() => setToolAccessLevel(connectionId, database, level))
	}

	function onAsk(connectionId: string, ask: boolean) {
		void persistAccess(() => setToolAccessAsk(connectionId, ask))
	}

	async function onEnable(on: boolean) {
		busy = true
		error = null
		try {
			mcp = await setMcpEnabled(on)
			await loadHarnesses()
		} catch (err) {
			error = errorMessage(err)
			await load()
		} finally {
			busy = false
		}
	}

	async function onRotate() {
		busy = true
		error = null
		try {
			mcp = await rotateMcpToken()
			await loadHarnesses()
		} catch (err) {
			error = errorMessage(err)
		} finally {
			busy = false
		}
	}

	async function onInstall(id: string) {
		busy = true
		busyId = id
		error = null
		try {
			const next = await installMcpHarness(id)
			harnesses = harnesses.map((h) => (h.id === next.id ? next : h))
		} catch (err) {
			error = errorMessage(err)
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
		Levels apply to MCP clients and the built-in agent: None hides a
		connection, Read runs SELECT and find, Write adds INSERT and UPDATE, Full
		adds DELETE, DDL and DROP. A database's level wins over its connection's,
		and a connection's over the default. A call above the level is refused
		with a message that says what to change here; nothing pops up.
	</p>
	<ControlGroup>
		<ControlRow label="MCP default" description={defaultHint}>
			<Segmented
				label="MCP default"
				value={access.mcpDefault}
				options={levelOptions}
				disabled={busy}
				onchange={onDefault}
			/>
		</ControlRow>
	</ControlGroup>
	{#if access.connections.length === 0}
		<p class="mt-3 mb-0 text-caption text-fg-muted">
			Save a connection first, then set its level here.
		</p>
	{:else}
		<div class="mt-3">
			<ToolAccessList
				connections={access.connections}
				{busy}
				onlevel={onLevel}
				onask={onAsk}
			/>
		</div>
	{/if}
</section>

{#if ready}
	<ControlGroup class="mt-4">
		<div class="px-3 py-3">
			<p class="m-0 mb-1.5 text-caption font-medium text-fg-secondary">URL</p>
			<div class="flex items-center gap-1.5">
				<code
					class="min-w-0 flex-1 truncate rounded-md border border-line bg-field px-2 py-1 font-mono text-caption text-fg"
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
					class="min-w-0 flex-1 truncate rounded-md border border-line bg-field px-2 py-1 font-mono text-caption text-fg"
					>{mcp.token}</code
				>
				<Button size="sm" onclick={() => void copy(mcp.token ?? '', 'token')}>
					{copied === 'token' ? 'Copied' : 'Copy'}
				</Button>
			</div>
		</div>
	</ControlGroup>

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
			<div class="overflow-hidden rounded-lg border border-line bg-surface">
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
			class="m-0 max-h-40 overflow-auto rounded-lg border border-line bg-surface p-3 font-mono text-caption text-fg">{snippet}</pre>
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
{#if error}
	<p class="mt-2 mb-0 text-caption text-danger" role="alert">{error}</p>
{/if}
