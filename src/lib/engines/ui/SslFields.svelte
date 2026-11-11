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

<ControlRow
	label="Require SSL / TLS"
	description="Fail instead of falling back to an unencrypted connection."
	icon="lock"
	inset={false}
>
	<Switch
		checked={draft.ssl}
		ariaLabel="Require SSL / TLS"
		{disabled}
		onchange={(on) => {
			draft.ssl = on
			if (on && (!draft.sslMode || draft.sslMode === 'disable')) draft.sslMode = 'require'
		}}
	/>
</ControlRow>

{#if extrasOpen}
	<div class="flex animate-rise flex-col gap-3.5 rounded-[10px] bg-hover p-4">
		<p class="m-0 text-label text-fg-muted">{verifyHint}</p>
		<Field label="SSL mode" error={modeError ?? undefined}>
			<Select
				value={mode === 'disable' ? 'require' : mode}
				options={modeOptions}
				{disabled}
				onchange={onMode}
			/>
		</Field>
		{#each [
			{ key: 'caCertPath', label: 'CA certificate', placeholder: 'Optional · RDS / private CA', title: 'Select CA certificate' },
			{ key: 'clientCertPath', label: 'Client certificate', placeholder: clientCertPlaceholder, title: 'Select client certificate' },
			{ key: 'clientKeyPath', label: 'Client key', placeholder: clientKeyPlaceholder, title: 'Select client key' },
		] as const as file (file.key)}
			<Field label={file.label} htmlFor="ssl-{file.key}">
				<div class="flex min-w-0 gap-2">
					<Input
						id="ssl-{file.key}"
						class="min-w-0 flex-1 font-mono"
						bind:value={draft[file.key]}
						placeholder={file.placeholder}
						{disabled}
					/>
					<Button class="shrink-0" {disabled} onclick={() => pickPath(file.key, file.title)}>
						Browse…
					</Button>
				</div>
			</Field>
		{/each}
		<Field label="Key passphrase" htmlFor="ssl-passphrase">
			<PasswordInput
				id="ssl-passphrase"
				bind:value={draft.sslClientKeyPassphrase}
				autocomplete="new-password"
				placeholder="Optional"
				{disabled}
			/>
		</Field>
	</div>
{/if}
