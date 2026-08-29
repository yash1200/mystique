<script lang="ts">
	import { Button, Modal } from '$core/ui'
	interface Props {
		open: boolean
		tabTitle: string
		tabKind: 'query' | 'table' | 'model' | 'structure'
		busy?: boolean
		onsave: () => void
		ondiscard: () => void
		oncancel: () => void
	}

	let {
		open,
		tabTitle,
		tabKind,
		busy = false,
		onsave,
		ondiscard,
		oncancel,
	}: Props = $props()

	const kindLabel = $derived(
		tabKind === 'query'
			? 'query'
			: tabKind === 'table'
				? 'table'
				: tabKind === 'structure'
					? 'structure'
					: 'tab',
	)
</script>

<Modal
	{open}
	onclose={oncancel}
	title="Unsaved changes"
	description="“{tabTitle}” has unsaved {kindLabel} changes. Save them before closing, discard them, or keep the tab open."
	dismissible={!busy}
>
	{#snippet footer()}
		<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
			Cancel
		</Button>
		<Button variant="ghost" size="sm" disabled={busy} onclick={ondiscard}>
			Discard
		</Button>
		<Button variant="primary" size="sm" disabled={busy} onclick={onsave}>
			{busy ? 'Saving…' : 'Save'}
		</Button>
	{/snippet}
</Modal>
