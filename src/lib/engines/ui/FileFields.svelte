<script lang="ts">
	import { Button, Field, Input } from '#core/ui'
	import type { EngineFieldsProps } from '../types'
	import { pickDatabaseFilePath } from './pick-database-file'

	let { draft = $bindable(), disabled = false }: EngineFieldsProps = $props()

	let picking = $state(false)
	let pickError = $state<string | null>(null)

	async function onBrowse() {
		pickError = null
		picking = true
		try {
			const result = await pickDatabaseFilePath({
				defaultPath: draft.path,
				title: 'Select database file',
			})
			if (result.ok) draft.path = result.path
			else if (!result.cancelled) pickError = result.error
		} finally {
			picking = false
		}
	}
</script>

<Field
	label="Database file"
	htmlFor="conn-path"
	error={pickError ?? undefined}
	hint="Mystique opens the file in place and creates it on first connect if it is missing."
>
	<div class="flex min-w-0 gap-2">
		<Input
			id="conn-path"
			class="min-w-0 flex-1 font-mono"
			bind:value={draft.path}
			placeholder="/path/to/database.db"
			{disabled}
			oninput={() => (pickError = null)}
		/>
		<Button class="shrink-0" disabled={picking || disabled} onclick={onBrowse}>
			{picking ? 'Opening…' : 'Browse…'}
		</Button>
	</div>
</Field>
