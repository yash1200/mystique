<script lang="ts">
	import { Button } from '$lib/components/ui'
	import { modalPanel, overlayFade } from '$lib/utils/motion'

	interface Props {
		open: boolean
		busy?: boolean
		onconfirm: () => void
		oncancel: () => void
	}

	let { open, busy = false, onconfirm, oncancel }: Props = $props()

	function onWindowKey(e: KeyboardEvent) {
		if (open && e.key === 'Escape' && !busy) oncancel()
	}
</script>

<svelte:window onkeydown={onWindowKey} />

{#if open}
	<div class="fixed inset-0 z-50 flex items-center justify-center p-4">
		<button
			type="button"
			class="absolute inset-0 border-none bg-black/45"
			aria-label="Dismiss"
			transition:overlayFade
			onclick={() => {
				if (!busy) oncancel()
			}}
			disabled={busy}
		></button>
		<div
			class="relative z-10 w-full max-w-md rounded-lg border border-line bg-elevated p-4 shadow-lg"
			transition:modalPanel
			role="dialog"
			aria-modal="true"
			aria-labelledby="leave-workspace-title"
		>
			<h2
				id="leave-workspace-title"
				class="m-0 text-body font-semibold text-fg"
			>
				Leave workspace?
			</h2>
			<p class="mt-2 mb-0 text-label leading-relaxed text-fg-secondary">
				This closes every open connection and returns to the home page.
			</p>
			<div class="mt-4 flex flex-wrap items-center justify-end gap-2">
				<Button variant="ghost" size="sm" disabled={busy} onclick={oncancel}>
					Cancel
				</Button>
				<Button
					variant="primary"
					size="sm"
					disabled={busy}
					onclick={onconfirm}
				>
					{busy ? 'Leaving…' : 'Leave workspace'}
				</Button>
			</div>
		</div>
	</div>
{/if}
