<script lang="ts">
	import {
		Button,
		Card,
		Checkbox,
		Field,
		FormGrid,
		Input,
		PasswordInput,
		Select,
	} from '$core/ui'
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

<Card title="SSH tunnel">
	<Checkbox
		bind:checked={draft.sshEnabled}
		label="Connect via SSH tunnel"
		{disabled}
	/>
	{#if draft.sshEnabled}
		<div class="mt-3">
		<FormGrid>
			<Field label="Bastion host">
				<Input bind:value={draft.sshHost} autocomplete="off" {disabled} />
			</Field>
			<Field label="Port">
				<Input
					type="number"
					bind:value={draft.sshPort}
					min="1"
					max="65535"
					{disabled}
				/>
			</Field>
			<Field label="User">
				<Input bind:value={draft.sshUser} autocomplete="off" {disabled} />
			</Field>
			<Field label="Auth">
				<Select
					bind:value={draft.sshAuth}
					options={authOptions}
					{disabled}
				/>
			</Field>
			{#if draft.sshAuth === 'password'}
				<Field label="SSH password" full>
					<PasswordInput
						bind:value={draft.sshPassword}
						autocomplete="off"
						placeholder={secretSet ? '••••••••' : 'Bastion password'}
						{disabled}
					/>
				</Field>
			{:else if draft.sshAuth === 'key'}
				<Field label="Private key" full>
					<div class="flex gap-2">
						<Input
							bind:value={draft.sshPrivateKeyPath}
							autocomplete="off"
							placeholder="~/.ssh/id_ed25519"
							class="flex-1"
							{disabled}
						/>
						<Button
							type="button"
							variant="secondary"
							onclick={pickKey}
							{disabled}>Browse</Button
						>
					</div>
				</Field>
				<Field label="Key passphrase" full>
					<PasswordInput
						bind:value={draft.sshPassphrase}
						autocomplete="new-password"
						placeholder={secretSet ? '••••••••' : 'Optional'}
						{disabled}
					/>
				</Field>
			{:else}
				<p class="col-span-full text-caption text-fg-muted">
					Uses the running SSH agent (`SSH_AUTH_SOCK`, or Pageant on Windows).
				</p>
			{/if}
		</FormGrid>
		</div>
		{#if secretSet}
			<button
				type="button"
				class="mt-2 text-caption text-fg-muted hover:text-danger hover:underline"
				onclick={() => {
					draft.sshPassword = ''
					draft.sshPassphrase = ''
					draft.clearStoredSshPassword = true
				}}
			>
				Clear stored SSH secret
			</button>
		{/if}
	{/if}
</Card>
