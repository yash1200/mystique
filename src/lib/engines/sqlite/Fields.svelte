<script lang="ts">
	import { Button, Card, Checkbox, Field, Input } from '$core/ui'
	import type { EngineFieldsProps } from '$engines'
	import { pickSqliteDatabasePath } from './pick-file'

	let { draft = $bindable() }: EngineFieldsProps = $props()

	let picking = $state(false)
	let pickError = $state<string | null>(null)

	async function onBrowse() {
		pickError = null
		picking = true
		try {
			const result = await pickSqliteDatabasePath(draft.path)
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
				aria-label="SQLite database file path"
				oninput={() => {
					pickError = null
				}}
			/>
			<Button
				type="button"
				variant="secondary"
				disabled={picking}
				onclick={onBrowse}
				class="shrink-0"
				aria-label="Browse for SQLite database file"
			>
				{picking ? 'Opening…' : 'Browse…'}
			</Button>
		</div>
	</Field>
	<div class="mt-3">
		<Checkbox bind:checked={draft.readOnly} label="Read-only" />
	</div>
</Card>
