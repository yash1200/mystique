<script lang="ts">
	import { Button } from '$core/ui'
	import { toastSlide } from '$core/util'
	import { appUpdates } from '../model.svelte'

	const update = $derived(appUpdates.available)
	const progress = $derived(appUpdates.progressLabel)
</script>

{#if appUpdates.visible && update}
	<aside
		class="fixed right-5 bottom-5 z-50 w-72 rounded-lg border border-line bg-elevated px-3.5 py-3 shadow-md"
		role="status"
		aria-live="polite"
		transition:toastSlide|global
	>
		<p class="m-0 text-label font-medium text-fg">
			Version {update.version} is available
		</p>
		<p class="mt-1 mb-0 text-caption text-fg-muted">
			{progress ?? 'A newer Mystique is ready to install.'}
		</p>
		{#if appUpdates.error}
			<p class="mt-1 mb-0 text-caption text-danger" role="alert">
				{appUpdates.error}
			</p>
		{/if}
		<div class="mt-3 flex justify-end gap-1.5">
			<Button
				size="sm"
				variant="ghost"
				disabled={appUpdates.busy}
				onclick={() => appUpdates.dismiss()}
			>
				Later
			</Button>
			<Button
				size="sm"
				variant="primary"
				disabled={appUpdates.busy}
				onclick={() => void appUpdates.install()}
			>
				{appUpdates.busy ? 'Installing…' : 'Install'}
			</Button>
		</div>
	</aside>
{/if}
