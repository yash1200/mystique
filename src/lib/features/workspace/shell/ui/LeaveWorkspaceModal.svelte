<script lang="ts">
	import { Button, Modal } from '#core/ui'
	import type { PendingLoss } from '#features/workspace/session'

	interface Props {
		/** What leaving would drop. The modal shows only while this is set. */
		loss: PendingLoss | null
		busy?: boolean
		onconfirm: () => void
		oncancel: () => void
	}

	let { loss, busy = false, onconfirm, oncancel }: Props = $props()
</script>

<Modal
	open={loss != null}
	onclose={oncancel}
	title="Leave workspace?"
	description="This closes every connection and returns to the home page. These will be lost:"
	dismissible={!busy}
>
	{#if loss}
		<ul class="list-disc space-y-1 pl-5 text-body">
			{#each loss.dirty as title, i (`dirty:${i}`)}
				<li>Unsaved changes in “{title}”</li>
			{/each}
			{#each loss.transactions as title, i (`txn:${i}`)}
				<li>Open transaction in “{title}” (rolled back)</li>
			{/each}
		</ul>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
			Cancel
		</Button>
		<Button variant="danger" size="sm" disabled={busy} onclick={onconfirm}>
			{busy ? 'Leaving…' : 'Leave and lose changes'}
		</Button>
	{/snippet}
</Modal>
