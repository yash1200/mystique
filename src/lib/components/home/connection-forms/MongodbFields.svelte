<script lang="ts">
	import { Card, Checkbox, Field, Input } from '$lib/components/ui'

	interface Props {
		uri?: string
		readOnly?: boolean
	}

	let { uri = $bindable(''), readOnly = $bindable(false) }: Props = $props()
</script>

<Card title="Connection">
	<Field label="Connection URI" full>
		<Input
			bind:value={uri}
			placeholder="mongodb://user:pass@127.0.0.1:27017/mydb?authSource=admin"
			autocomplete="off"
			class="font-mono"
		/>
	</Field>
	<div class="mt-3">
		<Checkbox bind:checked={readOnly} label="Read-only" />
	</div>
</Card>
