<script lang="ts">
	import { Button, Modal } from '#core/ui'
	import { resetGuard } from '../reset-guard.svelte'

	const loss = $derived(resetGuard.loss)
</script>

<Modal
	open={loss != null}
	onclose={() => resetGuard.cancel()}
	title="Reset Mystique?"
	description="This closes every connection and returns to the home page. These will be lost:"
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
		<Button variant="ghost" size="sm" onclick={() => resetGuard.cancel()}>
			Cancel
		</Button>
		<Button variant="danger" size="sm" onclick={() => resetGuard.confirm()}>
			Reset and lose changes
		</Button>
	{/snippet}
</Modal>
