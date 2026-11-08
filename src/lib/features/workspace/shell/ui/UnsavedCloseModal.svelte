<script lang="ts">
	import { Button, Modal } from '#core/ui'
	interface Props {
		open: boolean
		tabTitle: string
		tabKind: 'query' | 'table' | 'model' | 'structure' | 'metrics'
		busy?: boolean
		/** Last save failure. The tab stays open. */
		error?: string | null
		onsave: () => void
		ondiscard: () => void
		oncancel: () => void
	}

	let {
		open,
		tabTitle,
		tabKind,
		busy = false,
		error = null,
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
	{#if error}
		<p class="text-caption text-danger" role="alert">{error}</p>
	{/if}
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
