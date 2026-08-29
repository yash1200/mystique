<script lang="ts">
	import { Button, Modal } from '$core/ui'

	interface Props {
		open: boolean
		sessionLabel: string
		queryPreview?: string | null
		production?: boolean
		busy?: boolean
		onconfirm: () => void
		oncancel: () => void
	}

	let {
		open,
		sessionLabel,
		queryPreview = null,
		production = false,
		busy = false,
		onconfirm,
		oncancel,
	}: Props = $props()
</script>

<Modal
	{open}
	onclose={oncancel}
	title="End session"
	description={production
		? `This is a production connection. Ending ${sessionLabel} disconnects that client immediately.`
		: `End ${sessionLabel}? The client is disconnected immediately.`}
	dismissible={!busy}
>
	{#if queryPreview}
		<p
			class="m-0 max-h-24 overflow-auto font-mono text-caption leading-relaxed text-fg-secondary"
		>
			{queryPreview}
		</p>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
			Cancel
		</Button>
		<Button variant="danger" size="sm" disabled={busy} onclick={onconfirm}>
			{busy ? 'Ending…' : 'End session'}
		</Button>
	{/snippet}
</Modal>
