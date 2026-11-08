<script lang="ts">
	import { Card, Checkbox, Field, Input } from '#core/ui'
	import { adapterFor } from '../registry'
	import type { EngineFieldsProps } from '../types'

	let { draft = $bindable(), disabled = false }: EngineFieldsProps = $props()

	const adapter = $derived(adapterFor(draft.engine))
	const placeholder = $derived(
		adapter.connection.uriPlaceholder ??
			'mongodb://user:pass@127.0.0.1:27017/mydb?authSource=admin',
	)
</script>

<Card title="Connection">
	<Field label="Connection URI" full>
		<Input
			bind:value={draft.uri}
			{placeholder}
			autocomplete="off"
			class="font-mono"
			{disabled}
		/>
	</Field>
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
