<script lang="ts">
	import { Button, Input, Modal } from '$lib/components/core'

	interface Props {
		open: boolean
		initialName?: string
		busy?: boolean
		error?: string | null
		/** Dialog heading (e.g. Save query / Rename query). */
		title?: string
		description?: string
		confirmLabel?: string
		onconfirm: (name: string) => void
		oncancel: () => void
	}

	let {
		open,
		initialName = 'Untitled query',
		busy = false,
		error = null,
		title = 'Save query',
		description = 'Name this query for the current connection.',
		confirmLabel = 'Save',
		onconfirm,
		oncancel,
	}: Props = $props()

	// Seed via $effect when the dialog opens so prop updates are not frozen at mount.
	let name = $state('')

	$effect(() => {
		if (open) name = initialName || 'Untitled query'
	})

	function submit() {
		const n = name.trim()
		if (!n || busy) return
		onconfirm(n)
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
	<label
		class="mt-3 block text-caption font-medium text-fg-secondary"
		for="save-query-name"
	>
		Name
	</label>
	<div class="mt-1">
		<Input
			id="save-query-name"
			bind:value={name}
			inputSize="sm"
			disabled={busy}
			placeholder="e.g. Active users"
			autofocus
		/>
	</div>

	{#if error}
		<p class="mt-2 mb-0 text-caption text-danger" role="alert">{error}</p>
	{/if}

	{#snippet footer()}
		<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
			Cancel
		</Button>
		<Button
			variant="primary"
			size="sm"
			disabled={busy || !name.trim()}
			onclick={submit}
		>
			{busy ? 'Saving…' : confirmLabel}
		</Button>
	{/snippet}
</Modal>
