<script lang="ts">
	import {
		Button,
		ControlRow,
		Field,
		Input,
		PasswordInput,
		Select,
		Switch,
	} from '#core/ui'
	import type { EngineFieldsProps } from '../types'
	import { pickDatabaseFilePath } from './pick-database-file'

	let {
		draft = $bindable(),
		isEdit = false,
		sshPasswordSet = false,
		disabled = false,
	}: EngineFieldsProps & { sshPasswordSet?: boolean } = $props()

	const authOptions = [
		{ value: 'password', label: 'Password' },
		{ value: 'key', label: 'Private key' },
		{ value: 'agent', label: 'SSH agent' },
	]

	const secretSet = $derived(
		isEdit && sshPasswordSet && !draft.clearStoredSshPassword,
	)

	async function pickKey() {
		const result = await pickDatabaseFilePath({
			title: 'Select SSH private key',
			defaultPath: draft.sshPrivateKeyPath,
			filters: [{ name: 'All files', extensions: ['*'] }],
		})
		if (result.ok) draft.sshPrivateKeyPath = result.path
	}
</script>

<ControlRow
	label="Connect via SSH tunnel"
	description="Reach databases inside a private network through a bastion host."
	icon="terminal"
	inset={false}
>
	<Switch
		checked={draft.sshEnabled}
		ariaLabel="Connect via SSH tunnel"
		{disabled}
		onchange={(on) => (draft.sshEnabled = on)}
	/>
</ControlRow>

{#if draft.sshEnabled}
	<div
		class="grid animate-rise grid-cols-[repeat(auto-fit,minmax(min(12.5rem,100%),1fr))] gap-3.5 rounded-[10px] bg-hover p-4"
	>
		<Field label="SSH host" htmlFor="ssh-host">
			<Input
				id="ssh-host"
				class="font-mono"
				bind:value={draft.sshHost}
				placeholder="bastion.example.com"
				{disabled}
			/>
		</Field>
		<Field label="Port" htmlFor="ssh-port">
			<Input
				id="ssh-port"
				type="number"
				class="font-mono"
				bind:value={draft.sshPort}
				min="1"
				max="65535"
				{disabled}
			/>
		</Field>
		<Field label="User" htmlFor="ssh-user">
			<Input
				id="ssh-user"
				class="font-mono"
				bind:value={draft.sshUser}
				placeholder="ec2-user"
				{disabled}
			/>
		</Field>
		<Field label="Auth">
			<Select bind:value={draft.sshAuth} options={authOptions} ariaLabel="SSH auth" {disabled} />
		</Field>
		{#if draft.sshAuth === 'password'}
			<Field label="SSH password" htmlFor="ssh-password" full>
				<PasswordInput
					id="ssh-password"
					bind:value={draft.sshPassword}
					placeholder={secretSet ? '••••••••' : 'Bastion password'}
					{disabled}
				/>
			</Field>
		{:else if draft.sshAuth === 'key'}
			<Field label="Private key" htmlFor="ssh-key" full>
				<div class="flex min-w-0 gap-2">
					<Input
						id="ssh-key"
						class="min-w-0 flex-1 font-mono"
						bind:value={draft.sshPrivateKeyPath}
						placeholder="~/.ssh/id_ed25519"
						{disabled}
					/>
					<Button class="shrink-0" {disabled} onclick={pickKey}>Browse…</Button>
				</div>
			</Field>
			<Field label="Key passphrase" htmlFor="ssh-passphrase" full>
				<PasswordInput
					id="ssh-passphrase"
					bind:value={draft.sshPassphrase}
					autocomplete="new-password"
					placeholder={secretSet ? '••••••••' : 'Optional'}
					{disabled}
				/>
			</Field>
		{:else}
			<p class="col-span-full m-0 text-label text-fg-muted">
				Uses the running SSH agent (`SSH_AUTH_SOCK`, or Pageant on Windows).
			</p>
		{/if}
		{#if secretSet}
			<button
				type="button"
				class="col-span-full justify-self-start border-none bg-transparent p-0 text-label text-fg-muted hover:text-danger hover:underline"
				onclick={() => {
					draft.sshPassword = ''
					draft.sshPassphrase = ''
					draft.clearStoredSshPassword = true
				}}
			>
				Clear stored SSH secret
			</button>
		{/if}
	</div>
{/if}
