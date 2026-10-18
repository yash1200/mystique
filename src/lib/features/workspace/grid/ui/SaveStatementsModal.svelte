<script lang="ts">
	import { Button, Modal } from '#core/ui'

	interface Props {
		open: boolean
		statements: string[]
		busy?: boolean
		onconfirm: () => void
		oncancel: () => void
	}

	let { open, statements, busy = false, onconfirm, oncancel }: Props = $props()

	const countLabel = $derived(
		statements.length === 1 ? '1 statement' : `${statements.length} statements`,
	)
</script>

<Modal
	{open}
	onclose={oncancel}
	title="Save changes"
	description={countLabel}
	size="lg"
	dismissible={!busy}
	onkeydown={(e) => {
		if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && !busy) onconfirm()
	}}
>
	<pre
		class="mt-3 max-h-64 overflow-auto rounded-md border border-line bg-input px-3 py-2 font-mono text-caption leading-relaxed text-fg whitespace-pre-wrap"
	>{statements.join('\n')}</pre>

	{#snippet footer()}
		<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
			Cancel
		</Button>
		<Button variant="primary" size="sm" disabled={busy || statements.length === 0} onclick={onconfirm}>
			{busy ? 'Saving…' : 'Save'}
		</Button>
	{/snippet}
</Modal>
