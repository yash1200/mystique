<script lang="ts">
	import type { ParsedConnectionUrl } from '#core/contracts'
	import { Field, Input, PasswordInput } from '#core/ui'
	import { runtimeFor } from '../products'
	import { adapterFor } from '../registry'
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
	const passwordPlaceholder = $derived(
		!draft.savePassword
			? 'Asked on connect'
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

<ImportConnectionUrl engine={draft.engine} {disabled} onparsed={applyParsed} />

<div class="grid grid-cols-[minmax(0,3fr)_minmax(0,1fr)] gap-3.5">
	<Field label="Host" htmlFor="conn-host">
		<Input id="conn-host" class="font-mono" bind:value={draft.host} {disabled} />
	</Field>
	<Field label="Port" htmlFor="conn-port">
		<Input
			id="conn-port"
			type="number"
			class="font-mono"
			bind:value={draft.port}
			min="1"
			max="65535"
			{disabled}
		/>
	</Field>
</div>
<Field label="Database" htmlFor="conn-database">
	<Input id="conn-database" class="font-mono" bind:value={draft.database} {disabled} />
</Field>
<div class="grid grid-cols-[repeat(auto-fit,minmax(min(13.75rem,100%),1fr))] gap-3.5">
	<Field label="Username" htmlFor="conn-username">
		<Input
			id="conn-username"
			class="font-mono"
			bind:value={draft.username}
			autocomplete="username"
			{disabled}
		/>
	</Field>
	<Field label="Password" htmlFor="conn-password">
		<PasswordInput
			id="conn-password"
			bind:value={draft.password}
			autocomplete="new-password"
			placeholder={passwordPlaceholder}
			{disabled}
		/>
	</Field>
</div>
