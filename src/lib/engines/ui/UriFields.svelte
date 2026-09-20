<script lang="ts">
	import { Card, Checkbox, Field, Input } from '$core/ui'
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
	<div class="mt-3">
		<Checkbox bind:checked={draft.readOnly} label="Read-only" {disabled} />
	</div>
</Card>
