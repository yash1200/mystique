<script lang="ts">
	import type { ParsedConnectionUrl } from '#core/contracts'
	import {
		Card,
		Checkbox,
		Field,
		FormGrid,
		Input,
		PasswordInput,
	} from '#core/ui'
	import { adapterFor } from '../registry'
	import { runtimeFor } from '../products'
	import type { EngineFieldsProps } from '../types'
	import { applyParsedHostUrl } from './apply-parsed-url'
	import ImportConnectionUrl from './ImportConnectionUrl.svelte'

	let {
		draft = $bindable(),
		isEdit = false,
		passwordSet = false,
		disabled = false,
	}: EngineFieldsProps = $props()

	const adapter = $derived(adapterFor(draft.engine))
	const runtime = $derived(runtimeFor(draft.engine) ?? adapter.id)
	const port = $derived(
		Number(draft.port) || adapter.defaultPort || 5432,
	)
	const urlPlaceholder = $derived(
		runtime === 'mysql'
			? `mysql://user:pass@host:${port}/dbname`
			: `postgresql://user:pass@host:${port}/dbname?sslmode=require`,
	)
	const passwordPlaceholder = $derived(
		!draft.savePassword
			? 'Optional'
			: isEdit && passwordSet
				? '••••••••'
				: runtime === 'mysql'
					? '123'
					: runtime === 'postgres'
						? 'postgres'
						: 'Optional',
	)

	function applyParsed(parsed: ParsedConnectionUrl) {
		applyParsedHostUrl(draft, parsed)
	}
</script>

<Card title="Server">
	<ImportConnectionUrl
		engine={draft.engine}
		placeholder={urlPlaceholder}
		{disabled}
		onparsed={applyParsed}
	/>
	<FormGrid>
		<Field label="Host">
			<Input bind:value={draft.host} autocomplete="off" {disabled} />
		</Field>
		<Field label="Port">
			<Input
				type="number"
				bind:value={draft.port}
				min="1"
				max="65535"
				{disabled}
			/>
		</Field>
		<Field label="Database" full>
			<Input bind:value={draft.database} autocomplete="off" {disabled} />
		</Field>
		<Field label="Username">
			<Input
				bind:value={draft.username}
				autocomplete="username"
				{disabled}
			/>
		</Field>
		<Field label="Password">
			<PasswordInput
				bind:value={draft.password}
				autocomplete="new-password"
				placeholder={passwordPlaceholder}
				{disabled}
			/>
		</Field>
	</FormGrid>

	<div class="mt-3 flex flex-wrap gap-4">
		<Checkbox bind:checked={draft.readOnly} label="Read-only" {disabled} />
		<Checkbox
			bind:checked={draft.savePassword}
			label="Save password"
			{disabled}
		/>
	</div>
	{#if !draft.savePassword}
		<p class="m-0 mt-2 text-caption text-fg-muted">
			Mystique asks for the password once per session and keeps it in memory
			until you quit.
		</p>
	{/if}
</Card>
