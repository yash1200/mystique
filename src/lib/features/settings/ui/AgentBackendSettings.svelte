<script lang="ts">
	import { errorMessage } from '#core/ipc'
	import {
		Button,
		ControlGroup,
		ControlRow,
		Field,
		Icon,
		IconButton,
		Input,
		PasswordInput,
		Segmented,
	} from '#core/ui'
	import {
		acpSettings,
		joinArgs,
		looksSecret,
		splitArgs,
		type AcpProbe,
		type AcpSettingsView,
	} from '#features/workspace/agent'
	import { onMount } from 'svelte'

	/** Settings → AI → Agent: the built-in agent or an external ACP agent. */

	type EnvRow = { key: number; name: string; value: string; secret: boolean; saved: boolean }

	const KIND_OPTIONS = [
		{ value: 'builtin', label: 'Built-in' },
		{ value: 'acp', label: 'External (ACP)' },
	]

	let kind = $state('builtin')
	let command = $state('')
	let argsLine = $state('')
	let env = $state<EnvRow[]>([])
	let nextKey = 0
	let busy = $state(false)
	let testing = $state(false)
	let error = $state<string | null>(null)
	let saved = $state(false)
	let probe = $state<AcpProbe | null>(null)

	const view = $derived(acpSettings.view)

	function rowsFrom(next: AcpSettingsView): EnvRow[] {
		return next.env.map((e) => ({
			key: nextKey++,
			name: e.name,
			value: e.value ?? '',
			secret: e.secret,
			saved: e.saved,
		}))
	}

	function fill(next: AcpSettingsView) {
		kind = next.kind
		command = next.command
		argsLine = joinArgs(next.args)
		env = rowsFrom(next)
	}

	onMount(() => {
		acpSettings
			.load(true)
			.then((next) => {
				if (next) fill(next)
			})
			.catch((e: unknown) => (error = errorMessage(e)))
	})

	function input() {
		return {
			kind,
			command: command.trim(),
			args: splitArgs(argsLine),
			env: env
				.filter((row) => row.name.trim())
				.map((row) => ({ name: row.name.trim(), value: row.value })),
		}
	}

	async function save(): Promise<boolean> {
		busy = true
		error = null
		try {
			fill(await acpSettings.save(input()))
			saved = true
			setTimeout(() => (saved = false), 1400)
			return true
		} catch (e) {
			error = errorMessage(e)
			return false
		} finally {
			busy = false
		}
	}

	async function onKind(next: string) {
		if (next === kind) return
		kind = next
		probe = null
		// Built-in needs nothing else; External saves once a command is set.
		if (next === 'builtin' || command.trim()) await save()
	}

	/** Save, then start the agent and run `initialize`. */
	async function test() {
		probe = null
		if (!(await save())) return
		testing = true
		try {
			probe = await acpSettings.test()
		} catch (e) {
			error = errorMessage(e)
		} finally {
			testing = false
		}
	}

	function addRow() {
		env = [...env, { key: nextKey++, name: '', value: '', secret: false, saved: false }]
	}

	function removeRow(key: number) {
		env = env.filter((row) => row.key !== key)
	}

	function isSecretRow(row: EnvRow): boolean {
		return row.secret || looksSecret(row.name)
	}
</script>

<section class="mt-5">
	<ControlGroup>
		<ControlRow
			label="Agent"
			description="Built-in uses your provider keys. External runs an Agent Client Protocol agent, such as Claude Code, in the same panel."
		>
			<Segmented
				label="Agent"
				value={kind}
				options={KIND_OPTIONS}
				disabled={busy || !view}
				onchange={(v) => void onKind(v)}
			/>
		</ControlRow>
	</ControlGroup>

	{#if kind === 'acp'}
		<div
			class="mt-3 flex flex-col gap-3 rounded-lg border border-line bg-panel px-3 py-3"
		>
			<Field
				label="Command"
				htmlFor="acp-command"
				hint="Full path, or a name on PATH. Runs without a shell."
			>
				<Input
					id="acp-command"
					bind:value={command}
					inputSize="sm"
					spellcheck="false"
					autocomplete="off"
					placeholder="npx"
				/>
			</Field>
			<Field
				label="Arguments"
				htmlFor="acp-args"
				hint="Quote an argument that has spaces."
			>
				<Input
					id="acp-args"
					bind:value={argsLine}
					inputSize="sm"
					spellcheck="false"
					autocomplete="off"
					placeholder="-y @agentclientprotocol/claude-agent-acp"
				/>
			</Field>

			<div class="flex flex-col gap-1.5">
				<p class="m-0 text-label font-medium text-fg-secondary">Environment</p>
				{#each env as row (row.key)}
					<div class="flex items-center gap-1.5">
						<Input
							bind:value={row.name}
							inputSize="sm"
							spellcheck="false"
							autocomplete="off"
							placeholder="NAME"
							aria-label="Variable name"
							class="w-2/5 font-mono"
						/>
						{#if isSecretRow(row)}
							<PasswordInput
								bind:value={row.value}
								inputSize="sm"
								autocomplete="off"
								placeholder={row.saved ? 'Saved in the vault' : 'Value'}
								aria-label={`${row.name || 'Variable'} value`}
							/>
						{:else}
							<Input
								bind:value={row.value}
								inputSize="sm"
								spellcheck="false"
								autocomplete="off"
								placeholder="Value"
								aria-label={`${row.name || 'Variable'} value`}
							/>
						{/if}
						<IconButton
							label="Remove variable"
							variant="quiet"
							size="sm"
							onclick={() => removeRow(row.key)}
						>
							<Icon name="close" />
						</IconButton>
					</div>
				{/each}
				<div class="flex items-center justify-between gap-2">
					<Button size="sm" variant="ghost" onclick={addRow}>
						<Icon name="plus" />
						Add variable
					</Button>
					<span class="text-caption text-fg-muted">
						Names like *_KEY or *_TOKEN are kept in the vault.
					</span>
				</div>
			</div>

			<p class="m-0 text-caption leading-relaxed text-fg-muted">
				The agent gets only Mystique's database tools, through the MCP
				server (turn it on in Settings → MCP), with MCP's access levels
				and the approval dock. File and terminal access are declined; its
				other tools are refused.
			</p>

			<div class="flex items-center justify-end gap-1.5">
				<Button
					size="sm"
					variant="ghost"
					disabled={busy || testing || !command.trim()}
					onclick={() => void test()}
				>
					{testing ? 'Testing…' : 'Test'}
				</Button>
				<Button
					size="sm"
					disabled={busy || testing || !command.trim()}
					onclick={() => void save()}
				>
					{saved ? 'Saved' : 'Save'}
				</Button>
			</div>

			{#if probe}
				<div
					class="rounded-md border border-line bg-app px-3 py-2 text-caption text-fg-secondary"
					aria-live="polite"
				>
					<p class="m-0 font-medium text-fg">
						{probe.title || probe.name || 'Agent'}
						{#if probe.version}<span class="font-normal text-fg-muted">
								{probe.version}</span
							>{/if}
					</p>
					<p class="m-0 mt-1">
						ACP v{probe.protocolVersion} · MCP over HTTP: {probe.mcpHttp
							? 'yes'
							: 'no'}{probe.loadSession ? ' · resumes sessions' : ''}
					</p>
					{#if probe.authMethods.length > 0}
						<p class="m-0 mt-1">
							Sign-in: {probe.authMethods.join(', ')}. Sign in from a
							terminal, or set its API key above.
						</p>
					{/if}
					{#if probe.problem}
						<p class="m-0 mt-1 text-danger" role="alert">{probe.problem}</p>
					{/if}
				</div>
			{/if}
		</div>
	{/if}

	{#if error}
		<p class="mt-2 mb-0 text-caption whitespace-pre-wrap text-danger" role="alert">
			{error}
		</p>
	{/if}
</section>
