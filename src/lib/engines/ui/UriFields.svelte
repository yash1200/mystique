<script lang="ts">
	import { Field, Input } from '#core/ui'
	import { adapterFor } from '../registry'
	import type { EngineFieldsProps } from '../types'

	let { draft = $bindable(), disabled = false }: EngineFieldsProps = $props()

	const placeholder = $derived(
		adapterFor(draft.engine).connection.uriPlaceholder ??
			'mongodb://user:pass@127.0.0.1:27017/mydb?authSource=admin',
	)
</script>

<Field label="Connection URI" htmlFor="conn-uri">
	<Input
		id="conn-uri"
		class="font-mono text-data"
		bind:value={draft.uri}
		{placeholder}
		{disabled}
	/>
</Field>
