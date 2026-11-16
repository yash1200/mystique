<script lang="ts">
	/** Settings › About: update status and the one action that fits it. */
	import { Button, ControlRow, FetchedAgo } from '#core/ui'
	import { updater } from '../updater.svelte'

	const phase = $derived(updater.phase)

	const description = $derived.by(() => {
		switch (phase.kind) {
			case 'checking':
				return 'Checking for updates…'
			case 'current':
				return 'Mystique is up to date.'
			case 'available':
				return `Mystique ${phase.update.version} is available.`
			case 'downloading':
				return phase.total
					? `Downloading ${phase.update.version}… ${Math.round((phase.received / phase.total) * 100)}%`
					: `Downloading ${phase.update.version}…`
			case 'ready':
				return `Mystique ${phase.update.version} is installed. Restart to use it.`
			case 'failed':
				return phase.message
			default:
				return 'Mystique checks GitHub for a new version when it starts.'
		}
	})
</script>

<ControlRow label="Updates" {description}>
	<div class="flex shrink-0 items-center gap-2">
		{#if updater.checkedAt != null && (phase.kind === 'current' || phase.kind === 'failed')}
			<FetchedAgo at={updater.checkedAt} prefix="Checked" />
		{/if}
		{#if phase.kind === 'available' || (phase.kind === 'failed' && phase.update)}
			<Button size="sm" variant="primary" onclick={() => void updater.update()}>
				{phase.kind === 'failed' ? 'Try again' : 'Update'}
			</Button>
		{:else if phase.kind === 'ready'}
			<Button size="sm" variant="primary" onclick={() => void updater.restart()}>Restart now</Button>
		{:else}
			<Button
				size="sm"
				disabled={updater.busy}
				onclick={() => void updater.check({ manual: true })}
			>
				{phase.kind === 'checking' ? 'Checking…' : 'Check for updates'}
			</Button>
		{/if}
	</div>
</ControlRow>
