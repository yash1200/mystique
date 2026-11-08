<script lang="ts">
	import { Button, Modal } from '#core/ui'
	interface Props {
		open: boolean
		tabTitle: string
		busy?: boolean
		error?: string | null
		oncommit: () => void
		onrollback: () => void
		oncancel: () => void
	}

	let {
		open,
		tabTitle,
		busy = false,
		error = null,
		oncommit,
		onrollback,
		oncancel,
	}: Props = $props()
</script>

<Modal
	{open}
	onclose={oncancel}
	title="Open transaction"
	description="“{tabTitle}” has a transaction that is not committed. Commit or roll it back before closing, or keep the tab open."
	dismissible={!busy}
>
	{#if error}
		<p class="text-caption text-danger" role="alert">{error}</p>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
			Cancel
		</Button>
		<Button variant="ghost" size="sm" disabled={busy} onclick={onrollback}>
			Roll back
		</Button>
		<Button variant="primary" size="sm" disabled={busy} onclick={oncommit}>
			Commit
		</Button>
	{/snippet}
</Modal>
