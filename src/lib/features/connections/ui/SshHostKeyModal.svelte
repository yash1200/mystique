<script lang="ts">
	import { Alert, Button, Modal } from '#core/ui'
	import { sshHostKeys } from '../ssh-host-key.svelte'

	const prompt = $derived(sshHostKeys.prompt)
	const busy = $derived(sshHostKeys.busy)
	const mismatch = $derived(prompt?.kind === 'mismatch')
</script>

<Modal
	open={!!prompt}
	onclose={sshHostKeys.cancel}
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
		{#if sshHostKeys.error}
			<div class="mt-3">
				<Alert tone="danger">{sshHostKeys.error}</Alert>
			</div>
		{/if}
	{/if}
	{#snippet footer()}
		<Button variant="ghost" onclick={sshHostKeys.cancel} disabled={busy}>Cancel</Button>
		<Button
			variant={mismatch ? 'danger' : 'primary'}
			disabled={busy || !prompt}
			onclick={() => void sshHostKeys.accept()}
		>
			{mismatch ? 'Replace and continue' : 'Trust host'}
		</Button>
	{/snippet}
</Modal>
