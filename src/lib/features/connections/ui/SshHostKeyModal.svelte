<script lang="ts">
	import type { SshHostKeyPrompt } from '$core/contracts'
	import { Button, Modal } from '$core/ui'

	interface Props {
		prompt: SshHostKeyPrompt | null
		busy?: boolean
		oncancel: () => void
		onaccept: (prompt: SshHostKeyPrompt) => void | Promise<void>
	}

	let { prompt, busy = false, oncancel, onaccept }: Props = $props()

	const mismatch = $derived(prompt?.kind === 'mismatch')
</script>

<Modal
	open={!!prompt}
	onclose={oncancel}
	title={mismatch ? 'SSH host key changed' : 'Unknown SSH host key'}
	description={prompt
		? `${prompt.host}:${prompt.port}`
		: undefined}
	layout="inset"
	size="sm"
>
	{#if prompt}
		<p class="text-body text-fg">
			{#if mismatch}
				This bastion’s host key does not match the one Mystique stored. Someone
				could be intercepting the connection.
			{:else}
				Trust this host key to continue. Mystique will remember it on this
				machine.
			{/if}
		</p>
		<dl class="mt-3 space-y-1 font-mono text-caption text-fg-muted">
			<div><dt class="inline text-fg">Type</dt> {prompt.keyType}</div>
			<div class="break-all">
				<dt class="inline text-fg">SHA256</dt>
				{prompt.fingerprint}
			</div>
		</dl>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" onclick={oncancel} disabled={busy}>Cancel</Button>
		<Button
			variant={mismatch ? 'danger' : 'primary'}
			disabled={busy || !prompt}
			onclick={() => prompt && onaccept(prompt)}
		>
			{mismatch ? 'Replace and continue' : 'Trust host'}
		</Button>
	{/snippet}
</Modal>
