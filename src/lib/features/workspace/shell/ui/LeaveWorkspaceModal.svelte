<script lang="ts">
	import { Button, Modal } from '$core/ui'
	interface Props {
		open: boolean
		busy?: boolean
		onconfirm: () => void
		oncancel: () => void
	}

	let { open, busy = false, onconfirm, oncancel }: Props = $props()
</script>

<Modal
	{open}
	onclose={oncancel}
	title="Leave workspace?"
	description="This closes every open connection and returns to the home page."
	dismissible={!busy}
>
	{#snippet footer()}
		<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
			Cancel
		</Button>
		<Button variant="primary" size="sm" disabled={busy} onclick={onconfirm}>
			{busy ? 'Leaving…' : 'Leave workspace'}
		</Button>
	{/snippet}
</Modal>
