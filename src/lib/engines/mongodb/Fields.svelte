<script lang="ts">
	import { Card, Checkbox, Field, Input } from '$core/ui'
	import type { EngineFieldsProps } from '$engines'

	let { draft = $bindable() }: EngineFieldsProps = $props()
</script>

<Card title="Connection">
	<Field label="Connection URI" full>
		<Input
			bind:value={draft.uri}
			placeholder="mongodb://user:pass@127.0.0.1:27017/mydb?authSource=admin"
			autocomplete="off"
			class="font-mono"
		/>
	</Field>
	<div class="mt-3">
		<Checkbox bind:checked={draft.readOnly} label="Read-only" />
	</div>
</Card>
