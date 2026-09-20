<script lang="ts">
	import { Button, Card, Checkbox, Field, Input } from '$core/ui'
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
			if (result.ok) {
				draft.path = result.path
				return
			}
			if (!result.cancelled) {
				pickError = result.error
			}
		} finally {
			picking = false
		}
	}
</script>

<Card title="Database file">
	<Field label="File path" error={pickError ?? undefined} full>
		<div class="flex min-w-0 items-center gap-2">
			<Input
				bind:value={draft.path}
				placeholder="/path/to/database.sqlite"
				autocomplete="off"
				class="min-w-0 flex-1 font-mono"
				aria-label="Database file path"
				{disabled}
				oninput={() => {
					pickError = null
				}}
			/>
			<Button
				type="button"
				variant="secondary"
				disabled={picking || disabled}
				onclick={onBrowse}
				class="shrink-0"
				aria-label="Browse for database file"
			>
				{picking ? 'Opening…' : 'Browse…'}
			</Button>
		</div>
	</Field>
	<p class="text-caption text-fg-muted mt-2">
		If the file does not exist, Mystique creates it on first connect.
	</p>
	<div class="mt-3">
		<Checkbox bind:checked={draft.readOnly} label="Read-only" {disabled} />
	</div>
</Card>
