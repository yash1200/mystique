<script lang="ts">
	import { Button, Modal } from '#core/ui'

	interface Props {
		open: boolean
		chatTitle: string
		busy?: boolean
		onconfirm: () => void
		oncancel: () => void
	}

	let { open, chatTitle, busy = false, onconfirm, oncancel }: Props = $props()
</script>

<Modal
	{open}
	onclose={oncancel}
	title="Delete chat"
	description="Delete “{chatTitle}”? This cannot be undone."
	dismissible={!busy}
>
	{#snippet footer()}
		<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
			Cancel
		</Button>
		<Button variant="danger" size="sm" disabled={busy} onclick={onconfirm}>
			{busy ? 'Deleting…' : 'Delete'}
		</Button>
	{/snippet}
</Modal>
