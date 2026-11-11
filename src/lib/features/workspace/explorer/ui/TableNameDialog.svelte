<script lang="ts">
	import type { TableMeta } from '#core/contracts'
	import { Button, Icon, Input, Modal } from '#core/ui'
	import { untrack } from 'svelte'
	import { tableNameError } from '../table-name'

	/** Rename or duplicate a table: a new name, checked against the names already there. */
	interface Props {
		action: 'rename' | 'duplicate'
		table: TableMeta
		/** table, collection, view… */
		noun: string
		database: string
		/** Names already in the table's schema (case does not matter). */
		taken: readonly string[]
		onconfirm: (name: string) => void
		oncancel: () => void
	}

	let { action, table, noun, database, taken, onconfirm, oncancel }: Props = $props()

	const INPUT_ID = 'table-name-input'

	// The dialog mounts per prompt, so the first value is the one to edit.
	let name = $state(untrack(() => (action === 'duplicate' ? `${table.name}_copy` : table.name)))

	const rename = $derived(action === 'rename')
	const trimmed = $derived(name.trim())
	const unchanged = $derived(trimmed === table.name)
	const error = $derived(
		tableNameError({ action, current: table.name, next: name, noun, schema: table.schema, taken }),
	)
	const blocked = $derived(!!error || (rename && unchanged))

	function submit(e?: Event) {
		e?.preventDefault()
		if (!blocked) onconfirm(trimmed)
	}
</script>

<Modal
	open
	onclose={oncancel}
	icon={rename ? 'pencil' : 'copy'}
	title="{rename ? 'Rename' : 'Duplicate'} {noun}"
	description="{rename ? 'Rename' : 'Copy'} “{table.name}” on {database}."
	size="sm"
>
	<form id="table-name-form" class="m-0 flex flex-col gap-1.5" onsubmit={submit}>
		<label class="text-label font-medium text-fg-secondary" for={INPUT_ID}>New name</label>
		<Input
			id={INPUT_ID}
			class="font-mono"
			bind:value={name}
			invalid={!!error && trimmed.length > 0}
			autofocus
			onfocus={(e) => e.currentTarget.select()}
		/>
		{#if error && trimmed.length > 0}
			<span class="text-label text-danger" role="alert">{error}</span>
		{/if}
	</form>
	{#if rename}
		<div class="flex gap-2.5 rounded-[10px] bg-hover px-3 py-2.5 text-[0.78125rem] leading-normal text-fg-secondary">
			<span class="mt-0.5 flex text-info [&_svg]:size-3.5"><Icon name="info" /></span>
			<span>Saved queries and other code that use the old name won’t follow the rename.</span>
		</div>
	{/if}
	{#snippet footer()}
		<Button onclick={oncancel}>Cancel</Button>
		<Button type="submit" form="table-name-form" variant="primary" disabled={blocked}>
			{rename ? 'Rename' : 'Duplicate'}
		</Button>
	{/snippet}
</Modal>
