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
	import { SSL_MODE_OPTIONS, sslModeFromConfig } from '../ssl'
	import { pickDatabaseFilePath } from './pick-database-file'

	let {
		draft = $bindable(),
		disabled = false,
	}: EngineFieldsProps = $props()

	const extrasOpen = $derived(Boolean(draft.ssl))
	const mode = $derived(
		sslModeFromConfig(draft.ssl, draft.sslMode ?? 'require'),
	)
	const verifyHint = $derived(
		mode === 'verify-ca' || mode === 'verify-full'
			? 'Add a CA file for private CAs (RDS bundle). System roots are used if empty.'
			: 'Encrypts the connection. Pick Verify full to check the server certificate.',
	)

	const certFilters = [
		{
			name: 'Certificates',
			extensions: ['pem', 'crt', 'cer', 'der', 'p12', 'pfx', 'key'],
		},
		{ name: 'All files', extensions: ['*'] },
	]

	async function pickPath(
		key: 'caCertPath' | 'clientCertPath' | 'clientKeyPath',
		title: string,
	) {
		const result = await pickDatabaseFilePath({
			title,
			defaultPath: draft[key],
			filters: certFilters,
		})
		if (result.ok) draft[key] = result.path
	}

	function onMode(value: string) {
		const next = sslModeFromConfig(true, value)
		draft.sslMode = next
		draft.ssl = next !== 'disable'
	}
</script>

<Card title="SSL / TLS">
	<Checkbox bind:checked={draft.ssl} label="Require SSL" {disabled} />
	{#if extrasOpen}
		<p class="mt-2 text-caption text-fg-muted">{verifyHint}</p>
		<div class="mt-3">
			<FormGrid>
				<Field label="SSL mode" full>
					<Select
						value={mode === 'disable' ? 'require' : mode}
						options={SSL_MODE_OPTIONS}
						{disabled}
						onchange={onMode}
					/>
				</Field>
				<Field label="CA certificate" full>
					<div class="flex gap-2">
						<Input
							bind:value={draft.caCertPath}
							autocomplete="off"
							placeholder="Optional · RDS / private CA"
							class="flex-1 font-mono"
							{disabled}
						/>
						<Button
							type="button"
							variant="secondary"
							{disabled}
							onclick={() => pickPath('caCertPath', 'Select CA certificate')}
							>Browse</Button
						>
					</div>
				</Field>
				<Field label="Client certificate" full>
					<div class="flex gap-2">
						<Input
							bind:value={draft.clientCertPath}
							autocomplete="off"
							placeholder="Optional · PEM or PKCS#12"
							class="flex-1 font-mono"
							{disabled}
						/>
						<Button
							type="button"
							variant="secondary"
							{disabled}
							onclick={() =>
								pickPath('clientCertPath', 'Select client certificate')}
							>Browse</Button
						>
					</div>
				</Field>
				<Field label="Client key" full>
					<div class="flex gap-2">
						<Input
							bind:value={draft.clientKeyPath}
							autocomplete="off"
							placeholder="Optional · PEM key or .p12"
							class="flex-1 font-mono"
							{disabled}
						/>
						<Button
							type="button"
							variant="secondary"
							{disabled}
							onclick={() => pickPath('clientKeyPath', 'Select client key')}
							>Browse</Button
						>
					</div>
				</Field>
				<Field label="Key passphrase" full>
					<PasswordInput
						bind:value={draft.sslClientKeyPassphrase}
						autocomplete="new-password"
						placeholder="Optional"
						{disabled}
					/>
				</Field>
			</FormGrid>
		</div>
	{/if}
</Card>
