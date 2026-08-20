<script lang="ts">
	import { Button, Input, Modal, Select } from '$lib/components/core'

	interface Props {
		open: boolean
		value?: string
		error?: string | null
		busy?: boolean
		/** Dialog title (e.g. Create table). */
		title?: string
		description?: string
		/** Field label (e.g. Database / Schema / Table). */
		nameLabel?: string
		namePlaceholder?: string
		confirmLabel?: string
		/** Optional group (schema) picker for create-object. */
		showGroup?: boolean
		groupLabel?: string
		groupOptions?: { value: string; label: string }[]
		groupValue?: string
		oncancel: () => void
		onconfirm: (name: string, group?: string) => void
	}

	let {
		open,
		value = $bindable(''),
		error = null,
		busy = false,
		title = 'Create database',
		description = 'Choose a name for the new database on this server.',
		nameLabel = 'Name',
		namePlaceholder = 'my_database',
		confirmLabel = 'Create',
		showGroup = false,
		groupLabel = 'Schema',
		groupOptions = [],
		groupValue = $bindable(''),
		oncancel,
		onconfirm,
	}: Props = $props()

	$effect(() => {
		if (open) {
			value = ''
			if (showGroup && groupOptions.length > 0 && !groupValue) {
				groupValue = groupOptions[0]!.value
			}
		}
	})

	function submit() {
		const name = value.trim()
		if (!name || busy) return
		onconfirm(name, showGroup ? groupValue : undefined)
	}
</script>

<Modal
	{open}
	onclose={oncancel}
	{title}
	{description}
	dismissible={!busy}
	onkeydown={(e) => {
		if (e.key === 'Enter' && !busy) submit()
	}}
>
	<div class="mt-3">
		{#if showGroup && groupOptions.length > 0}
			<label
				class="mb-1 block text-caption font-medium text-fg-secondary"
				for="create-catalog-group"
			>
				{groupLabel}
			</label>
			<div class="mb-3">
				<Select
					id="create-catalog-group"
					value={groupValue}
					options={groupOptions}
					inputSize="sm"
					disabled={busy}
					onchange={(v) => {
						groupValue = v
					}}
				/>
			</div>
		{/if}

		<label
			class="block text-caption font-medium text-fg-secondary"
			for="create-catalog-name"
		>
			{nameLabel}
		</label>
		<div class="mt-1">
			<Input
				id="create-catalog-name"
				bind:value
				placeholder={namePlaceholder}
				inputSize="sm"
				autocomplete="off"
				disabled={busy}
				autofocus
				class="font-mono"
			/>
		</div>

		{#if error}
			<p class="mt-2 mb-0 text-caption text-danger" role="alert">
				{error}
			</p>
		{/if}
	</div>

	{#snippet footer()}
		<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
			Cancel
		</Button>
		<Button
			variant="primary"
			size="sm"
			disabled={busy || !value.trim()}
			onclick={submit}
		>
			{confirmLabel}
		</Button>
	{/snippet}
</Modal>
