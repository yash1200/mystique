<script lang="ts">
	import { errorMessage } from '#core/ipc'
	import { Badge, Button, Icon, IconButton, Pill } from '#core/ui'
	import { copyTextToClipboard, isMacPlatform } from '#core/util'
	import { showToast } from '#features/toast'
	import {
		acpSettings,
		BUILTIN_AGENT,
		canAdd,
		groupDetected,
		probeLine,
		statusLabel,
		type AcpAgentView,
		type AcpProbe,
		type DetectedAcpAgent,
	} from '#features/workspace/agent'
	import { onMount } from 'svelte'
	import AcpAgentForm from './AcpAgentForm.svelte'

	/**
	 * Settings → AI → Agent: the built-in agent, the external (ACP) agents
	 * you added, what was found on this machine (Add with one click), what
	 * is not installed (with how to install it), and Custom agent….
	 */

	type Result = { probe: AcpProbe | null; error: string | null }

	const view = $derived(acpSettings.view)
	const groups = $derived(groupDetected(acpSettings.detected ?? []))
	const defaultAgent = $derived(view?.defaultAgent ?? BUILTIN_AGENT)
	const machine = isMacPlatform() ? 'this Mac' : 'this computer'

	let error = $state<string | null>(null)
	let busy = $state<string | null>(null)
	let results = $state<Record<string, Result>>({})
	/** `new` for Custom agent…, an agent id for Edit, or closed. */
	let form = $state<string | null>(null)
	let showMissing = $state(false)

	onMount(() => {
		acpSettings.load(true).catch((e: unknown) => (error = errorMessage(e)))
		if (!acpSettings.detected) void rescan()
	})

	async function rescan() {
		error = null
		try {
			await acpSettings.detect()
		} catch (e) {
			error = errorMessage(e)
		}
	}

	async function run(key: string, task: () => Promise<void>) {
		busy = key
		error = null
		try {
			await task()
		} catch (e) {
			error = errorMessage(e)
		} finally {
			busy = null
		}
	}

	async function test(id: string) {
		results = { ...results, [id]: { probe: null, error: null } }
		await run(`test:${id}`, async () => {
			try {
				const probe = await acpSettings.test(id)
				results = { ...results, [id]: { probe, error: null } }
			} catch (e) {
				results = { ...results, [id]: { probe: null, error: errorMessage(e) } }
			}
		})
	}

	/** Add a detected agent, then test it right away. */
	function add(agent: DetectedAcpAgent) {
		void run(`add:${agent.catalogId}`, async () => {
			const saved = await acpSettings.add(agent.catalogId)
			void test(saved.id)
		})
	}

	function remove(agent: AcpAgentView) {
		void run(`remove:${agent.id}`, async () => {
			await acpSettings.remove(agent.id)
			const next = { ...results }
			delete next[agent.id]
			results = next
			if (form === agent.id) form = null
		})
	}

	function makeDefault(id: string) {
		void run(`default:${id}`, () => acpSettings.setDefault(id))
	}

	function onsaved(id: string, andTest: boolean) {
		form = null
		if (andTest) void test(id)
	}

	async function copy(text: string, what: string) {
		const done = await copyTextToClipboard(text)
		if (done.ok) showToast('info', `Copied the ${what}.`)
		else showToast('danger', `Could not copy the ${what}.`)
	}

	function commandLine(agent: AcpAgentView): string {
		return [agent.command, ...agent.args].join(' ')
	}

	const editing = $derived(
		form && form !== 'new' ? (view?.agents.find((a) => a.id === form) ?? null) : null,
	)
</script>

{#snippet result(id: string)}
	{@const r = results[id]}
	{#if busy === `test:${id}`}
		<p class="m-0 mt-1 text-caption text-fg-muted" aria-live="polite">Testing…</p>
	{:else if r?.error}
		<p class="m-0 mt-1 text-caption whitespace-pre-wrap text-danger" role="alert">
			{r.error}
		</p>
	{:else if r?.probe}
		<div class="mt-1 text-caption text-fg-secondary" aria-live="polite">
			<p class="m-0">
				<span class="font-medium text-fg">{r.probe.title || r.probe.name || 'Agent'}</span>
				{#if r.probe.version}<span class="text-fg-muted"> {r.probe.version}</span>{/if}
				· {probeLine(r.probe)}
			</p>
			{#if r.probe.problem}
				<p class="m-0 mt-0.5 text-danger" role="alert">{r.probe.problem}</p>
			{/if}
		</div>
	{/if}
{/snippet}

<section class="mt-5 flex flex-col gap-4" aria-label="Agent">
	<div>
		<p class="m-0 text-label font-medium text-fg">Agent</p>
		<p class="m-0 mt-0.5 text-caption leading-relaxed text-fg-muted">
			New chats use the default. Switch any chat from the agent panel's header.
			External agents get only Mystique's database tools, through the MCP server
			(turn it on in Settings → MCP), with its access levels and the approval dock.
			File and terminal access are declined; their other tools are refused.
		</p>
	</div>

	<ul
		class="m-0 flex list-none flex-col divide-y divide-line rounded-lg border border-line bg-panel p-0"
		aria-label="Your agents"
	>
		<li class="flex items-center gap-3 px-3 py-2.5">
			<Badge tone="accent">Mq</Badge>
			<div class="min-w-0 flex-1">
				<p class="m-0 truncate text-label font-medium text-fg">Built-in</p>
				<p class="m-0 truncate text-caption text-fg-muted">Uses your provider keys</p>
			</div>
			{#if defaultAgent === BUILTIN_AGENT}
				<Pill tone="success">Default</Pill>
			{:else}
				<Button
					size="sm"
					variant="ghost"
					disabled={busy !== null}
					onclick={() => makeDefault(BUILTIN_AGENT)}
				>
					Make default
				</Button>
			{/if}
		</li>
		{#each view?.agents ?? [] as agent (agent.id)}
			<li class="px-3 py-2.5">
				<div class="flex items-center gap-3">
					<Badge>{agent.label.slice(0, 2)}</Badge>
					<div class="min-w-0 flex-1">
						<p class="m-0 truncate text-label font-medium text-fg">{agent.label}</p>
						<p
							class="m-0 truncate font-mono text-caption text-fg-muted"
							title={commandLine(agent)}
						>
							{commandLine(agent)}
						</p>
					</div>
					{#if defaultAgent === agent.id}
						<Pill tone="success">Default</Pill>
					{:else}
						<Button
							size="sm"
							variant="ghost"
							disabled={busy !== null}
							onclick={() => makeDefault(agent.id)}
						>
							Make default
						</Button>
					{/if}
					<Button
						size="sm"
						variant="ghost"
						disabled={busy !== null}
						onclick={() => void test(agent.id)}
					>
						Test
					</Button>
					<IconButton
						label={`Edit ${agent.label}`}
						variant="quiet"
						size="sm"
						onclick={() => (form = form === agent.id ? null : agent.id)}
					>
						<Icon name="settings" />
					</IconButton>
					<IconButton
						label={`Remove ${agent.label}`}
						variant="quiet"
						size="sm"
						class="hover:bg-danger/10 hover:text-danger"
						disabled={busy !== null}
						onclick={() => remove(agent)}
					>
						<Icon name="trash" />
					</IconButton>
				</div>
				{@render result(agent.id)}
			</li>
		{/each}
	</ul>

	{#if editing}
		{#key editing.id}
			<AcpAgentForm agent={editing} {onsaved} oncancel={() => (form = null)} />
		{/key}
	{/if}

	<div>
		<div class="flex items-center justify-between gap-2">
			<p class="m-0 text-label font-medium text-fg">Detected on {machine}</p>
			<Button
				size="sm"
				variant="ghost"
				disabled={acpSettings.detecting}
				onclick={() => void rescan()}
			>
				<Icon name="refresh" />
				{acpSettings.detecting ? 'Scanning…' : 'Rescan'}
			</Button>
		</div>
		{#if acpSettings.detected === null}
			<p class="m-0 mt-1 text-caption text-fg-muted" aria-live="polite">
				{acpSettings.detecting ? `Looking for agents on ${machine}…` : 'Not scanned yet.'}
			</p>
		{:else if groups.found.length === 0}
			<p class="m-0 mt-1 text-caption text-fg-muted">
				No known ACP agent was found. Install one below, or add a Custom agent.
			</p>
		{:else}
			<ul
				class="m-0 mt-1.5 flex list-none flex-col divide-y divide-line rounded-lg border border-line bg-panel p-0"
				aria-label="Detected agents"
			>
				{#each groups.found as agent (agent.catalogId)}
					<li class="px-3 py-2.5" data-status={agent.status}>
						<div class="flex items-center gap-3">
							<Badge>{agent.mark}</Badge>
							<div class="min-w-0 flex-1">
								<p class="m-0 flex items-baseline gap-1.5 text-label font-medium text-fg">
									<span class="truncate">{agent.name}</span>
									{#if agent.version}
										<span class="truncate text-caption font-normal text-fg-muted">
											{agent.version}
										</span>
									{/if}
								</p>
								<p
									class="m-0 truncate font-mono text-caption text-fg-muted"
									title={agent.path ?? ''}
								>
									{agent.path ?? ''}
								</p>
							</div>
							<Pill tone={agent.status === 'unconfirmed' ? 'warning' : 'neutral'}>
								{statusLabel(agent)}
							</Pill>
							{#if agent.addedId}
								<Pill tone="success">Added</Pill>
							{:else if canAdd(agent)}
								<Button
									size="sm"
									disabled={busy !== null}
									onclick={() => add(agent)}
								>
									{busy === `add:${agent.catalogId}` ? 'Adding…' : 'Add'}
								</Button>
							{/if}
						</div>
						{#if agent.note}
							<p class="m-0 mt-1 text-caption text-fg-muted">{agent.note}</p>
						{/if}
						{#if agent.addedId}
							{@render result(agent.addedId)}
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</div>

	{#if groups.missing.length > 0}
		<div>
			<button
				type="button"
				class="flex w-full items-center gap-1.5 border-0 bg-transparent p-0 text-left text-label font-medium text-fg-secondary hover:text-fg"
				aria-expanded={showMissing}
				onclick={() => (showMissing = !showMissing)}
			>
				<Icon name={showMissing ? 'chevron-down' : 'chevron-right'} />
				Not installed ({groups.missing.length})
			</button>
			{#if showMissing}
				<ul
					class="m-0 mt-1.5 flex list-none flex-col divide-y divide-line rounded-lg border border-line bg-panel p-0"
					aria-label="Agents not installed"
				>
					{#each groups.missing as agent (agent.catalogId)}
						<li class="flex items-center gap-3 px-3 py-2">
							<Badge>{agent.mark}</Badge>
							<div class="min-w-0 flex-1">
								<p class="m-0 truncate text-label font-medium text-fg">{agent.name}</p>
								<p
									class="m-0 truncate font-mono text-caption text-fg-muted"
									title={agent.install}
								>
									{agent.install}
								</p>
							</div>
							<Button
								size="sm"
								variant="ghost"
								onclick={() => void copy(agent.install, 'install command')}
							>
								Copy command
							</Button>
							<Button
								size="sm"
								variant="ghost"
								onclick={() => void copy(agent.homepage, 'link')}
							>
								Copy link
							</Button>
						</li>
					{/each}
				</ul>
				<p class="m-0 mt-1 text-caption text-fg-muted">
					Mystique never installs anything. Install an agent in a terminal, then
					Rescan.
				</p>
			{/if}
		</div>
	{/if}

	<div>
		{#if form === 'new'}
			<AcpAgentForm agent={null} {onsaved} oncancel={() => (form = null)} />
		{:else}
			<Button size="sm" variant="ghost" onclick={() => (form = 'new')}>
				<Icon name="plus" />
				Custom agent…
			</Button>
		{/if}
	</div>

	{#if error}
		<p class="m-0 text-caption whitespace-pre-wrap text-danger" role="alert">{error}</p>
	{/if}
</section>
