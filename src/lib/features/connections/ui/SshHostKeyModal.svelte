<script lang="ts">
	import { Alert, Button, Icon, Modal, Spinner } from '#core/ui'
	import { cn } from '#core/util'
	import { sshHostKeys } from '../ssh-host-key.svelte'

	const TITLE_ID = 'ssh-host-key-title'

	const prompt = $derived(sshHostKeys.prompt)
	const busy = $derived(sshHostKeys.busy)
	const mismatch = $derived(prompt?.kind === 'mismatch')
</script>

<Modal open={!!prompt} onclose={sshHostKeys.cancel} labelledby={TITLE_ID} size="sm">
	{#if prompt}
		<div class="flex flex-col gap-4">
			<header class="flex items-center gap-3">
				<span
					class={cn(
						'grid size-8 shrink-0 place-items-center rounded-[9px] [&_svg]:size-4',
						mismatch ? 'bg-danger-soft text-danger' : 'bg-hover text-fg-secondary',
					)}
				>
					<Icon name={mismatch ? 'shield-alert' : 'shield'} />
				</span>
				<div class="min-w-0">
					<h2 id={TITLE_ID} class="m-0 text-heading font-semibold tracking-tight">
						{mismatch ? 'SSH host key changed' : 'Unknown SSH host key'}
					</h2>
					<p class="m-0 truncate font-mono text-label text-fg-muted">
						{prompt.host}:{prompt.port}
					</p>
				</div>
			</header>

			{#if mismatch}
				<Alert tone="danger">
					This bastion’s host key does not match the one Mystique stored.
					Someone could be intercepting the connection.
				</Alert>
			{:else}
				<p class="m-0 text-fg-secondary">
					Trust this host key to continue. Mystique will remember it on this machine.
				</p>
			{/if}

			<dl class="m-0 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 rounded-md bg-hover px-3.5 py-3">
				<dt class="label-caps leading-[1.6]">Type</dt>
				<dd class="m-0 font-mono text-data select-text">{prompt.keyType}</dd>
				<dt class="label-caps leading-[1.6]">SHA256</dt>
				<dd class="m-0 font-mono text-data break-all select-text">{prompt.fingerprint}</dd>
			</dl>

			{#if sshHostKeys.error}
				<Alert tone="danger">{sshHostKeys.error}</Alert>
			{/if}
		</div>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" onclick={sshHostKeys.cancel} disabled={busy}>Cancel</Button>
		<Button
			variant={mismatch ? 'danger' : 'primary'}
			disabled={busy || !prompt}
			onclick={() => void sshHostKeys.accept()}
		>
			{#if busy}<Spinner size="sm" />{/if}
			{mismatch ? 'Replace and continue' : 'Trust host'}
		</Button>
	{/snippet}
</Modal>
