<script lang="ts">
	import { errorMessage } from '#core/ipc'
	import { Button, Field, Icon, IconButton, Input, PasswordInput } from '#core/ui'
	import {
		acpSettings,
		joinArgs,
		looksSecret,
		splitArgs,
		type AcpAgentView,
	} from '#features/workspace/agent'
	import { untrack } from 'svelte'

	/**
	 * The manual agent form (Custom agent… and Edit): command, arguments,
	 * environment. Secret-looking values are sealed in the vault.
	 */

	interface Props {
		/** The agent to change; `null` adds a custom agent. */
		agent: AcpAgentView | null
		/** Saved; `test` asks the parent to run Test on it. */
		onsaved: (id: string, test: boolean) => void
		oncancel: () => void
	}

	let { agent, onsaved, oncancel }: Props = $props()

	type EnvRow = { key: number; name: string; value: string; secret: boolean; saved: boolean }

	// The form edits a copy; the parent remounts it for another agent.
	const start = untrack(() => agent)
	let nextKey = 0
	let command = $state(start?.command ?? '')
	let argsLine = $state(joinArgs(start?.args ?? []))
	let env = $state<EnvRow[]>(
		(start?.env ?? []).map((e) => ({
			key: nextKey++,
			name: e.name,
			value: e.value ?? '',
			secret: e.secret,
			saved: e.saved,
		})),
	)
	let busy = $state(false)
	let error = $state<string | null>(null)

	async function save(test: boolean) {
		busy = true
		error = null
		try {
			const saved = await acpSettings.save({
				id: start?.id ?? null,
				catalogId: start?.catalogId ?? null,
				label: null,
				command: command.trim(),
				args: splitArgs(argsLine),
				env: env
					.filter((row) => row.name.trim())
					.map((row) => ({ name: row.name.trim(), value: row.value })),
				makeDefault: false,
			})
			onsaved(saved.id, test)
		} catch (e) {
			error = errorMessage(e)
		} finally {
			busy = false
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

	const idPrefix = `acp-${start?.id ?? 'new'}`
</script>

<div class="flex flex-col gap-3 rounded-lg border border-line bg-panel px-3 py-3">
	<p class="m-0 text-label font-medium text-fg">
		{start ? `Edit ${start.label}` : 'Custom agent'}
	</p>
	<Field
		label="Command"
		htmlFor={`${idPrefix}-command`}
		hint="Full path, or a name on PATH. Runs without a shell."
	>
		<Input
			id={`${idPrefix}-command`}
			bind:value={command}
			inputSize="sm"
			spellcheck="false"
			autocomplete="off"
			placeholder="npx"
		/>
	</Field>
	<Field
		label="Arguments"
		htmlFor={`${idPrefix}-args`}
		hint="Quote an argument that has spaces."
	>
		<Input
			id={`${idPrefix}-args`}
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

	{#if error}
		<p class="m-0 text-caption whitespace-pre-wrap text-danger" role="alert">{error}</p>
	{/if}

	<div class="flex items-center justify-end gap-1.5">
		<Button size="sm" variant="ghost" disabled={busy} onclick={oncancel}>Cancel</Button>
		<Button
			size="sm"
			variant="ghost"
			disabled={busy || !command.trim()}
			onclick={() => void save(true)}
		>
			Save and test
		</Button>
		<Button size="sm" disabled={busy || !command.trim()} onclick={() => void save(false)}>
			Save
		</Button>
	</div>
</div>
