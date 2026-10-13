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
	} from '#core/ui'
	import type { EngineFieldsProps } from '../types'
	import { runtimeFor } from '../products'
	import { SSL_MODE_OPTIONS, sslModeFromConfig, sslModeRejection } from '../ssl'
	import { pickDatabaseFilePath } from './pick-database-file'

	let {
		draft = $bindable(),
		disabled = false,
	}: EngineFieldsProps = $props()

	const extrasOpen = $derived(Boolean(draft.ssl))
	const mode = $derived(
		sslModeFromConfig(draft.ssl, draft.sslMode ?? 'require'),
	)
	const runtime = $derived(runtimeFor(String(draft.engine)) ?? '')
	const modeOptions = $derived(
		SSL_MODE_OPTIONS.filter((option) => {
			if (runtime === 'postgres') return true
			if (option.value === 'prefer') return false
			if (runtime === 'mongodb' && option.value === 'verify-ca') return false
			return true
		}),
	)
	const modeError = $derived(sslModeRejection(String(draft.engine), mode))
	const verifyHint = $derived(
		mode === 'verify-ca' || mode === 'verify-full'
			? 'Needs a CA file. The connection trusts that CA only, not system roots.'
			: 'Encrypts the connection. Pick Verify full to check the server certificate and its name.',
	)
	const clientCertPlaceholder = $derived(
		runtime === 'mysql'
			? 'Optional · PKCS#12 (.p12 or .pfx)'
			: runtime === 'mongodb'
				? 'Optional · one PEM with the certificate and the key'
				: 'Optional · PEM certificate',
	)
	const clientKeyPlaceholder = $derived(
		runtime === 'mysql'
			? 'Leave empty when the PKCS#12 file includes the key'
			: runtime === 'mongodb'
				? 'PEM key, or leave empty if it is already in the certificate file'
				: 'Optional · unencrypted PKCS#8 PEM',
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
	<Checkbox
		bind:checked={draft.ssl}
		label="Require SSL"
		{disabled}
		onchange={() => {
			if (draft.ssl && (!draft.sslMode || draft.sslMode === 'disable')) {
				draft.sslMode = 'require'
			}
		}}
	/>
	{#if extrasOpen}
		<p class="mt-2 text-caption text-fg-muted">{verifyHint}</p>
		<div class="mt-3">
			<FormGrid>
				<Field label="SSL mode" full>
					<Select
						value={mode === 'disable' ? 'require' : mode}
						options={modeOptions}
						{disabled}
						onchange={onMode}
					/>
				</Field>
				{#if modeError}
					<p class="text-caption text-danger" role="alert">{modeError}</p>
				{/if}
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
							placeholder={clientCertPlaceholder}
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
							placeholder={clientKeyPlaceholder}
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
