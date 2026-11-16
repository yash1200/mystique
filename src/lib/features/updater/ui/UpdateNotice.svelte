<script lang="ts">
	/** Bottom-right update card: offer, download progress, restart prompt. */
	import { Button, Icon, IconButton, type IconName } from '#core/ui'
	import { cn, toastSlide } from '#core/util'
	import { releaseUrl } from '../backend'
	import { updater } from '../updater.svelte'

	const phase = $derived(updater.phase)
	const shown = $derived(
		updater.noticeOpen &&
			(phase.kind === 'available' ||
				phase.kind === 'downloading' ||
				phase.kind === 'ready' ||
				(phase.kind === 'failed' && phase.update != null)),
	)
	const update = $derived('update' in phase ? phase.update : null)

	const percent = $derived(
		phase.kind === 'downloading' && phase.total
			? Math.min(100, Math.round((phase.received / phase.total) * 100))
			: null,
	)

	const head = $derived.by((): { icon: IconName; ink: string; title: string } => {
		switch (phase.kind) {
			case 'downloading':
				return { icon: 'download', ink: 'text-accent-text', title: 'Downloading update' }
			case 'ready':
				return { icon: 'check', ink: 'text-accent-text', title: 'Update ready' }
			case 'failed':
				return { icon: 'alert-circle', ink: 'text-danger', title: 'Update failed' }
			default:
				return { icon: 'download', ink: 'text-accent-text', title: 'Update available' }
		}
	})

	function megabytes(bytes: number): string {
		return (bytes / 1024 / 1024).toFixed(1)
	}
</script>

{#if shown && update}
	<section
		class="w-80 max-w-[calc(100vw-2.5rem)] rounded-lg bg-raised p-3.5 text-body text-fg shadow-lg"
		aria-labelledby="update-notice-title"
		aria-live="polite"
		transition:toastSlide|global
	>
		<div class="flex items-start gap-2.5">
			<span
				class={cn('grid size-7 shrink-0 place-items-center rounded-md bg-hover [&_svg]:size-3.5', head.ink)}
				aria-hidden="true"
			>
				<Icon name={head.icon} />
			</span>
			<div class="min-w-0 flex-1 pt-0.5">
				<h2 id="update-notice-title" class="m-0 text-body font-semibold">{head.title}</h2>
				<p class="mt-0.5 mb-0 text-label leading-relaxed text-fg-secondary">
					{#if phase.kind === 'available'}
						Mystique <span class="font-mono">{update.version}</span> is available. You have
						<span class="font-mono">{update.currentVersion}</span>.
					{:else if phase.kind === 'downloading'}
						Mystique <span class="font-mono">{update.version}</span> is downloading. You can keep working.
					{:else if phase.kind === 'ready'}
						Restart Mystique to finish updating to <span class="font-mono">{update.version}</span>.
					{:else if phase.kind === 'failed'}
						{phase.message}
					{/if}
				</p>
			</div>
			<IconButton
				label={phase.kind === 'downloading' ? 'Hide; the download continues' : 'Close'}
				variant="quiet"
				size="sm"
				onclick={updater.dismiss}
			>
				<Icon name="close" />
			</IconButton>
		</div>

		{#if phase.kind === 'downloading'}
			<div class="mt-3 pl-9.5">
				<div
					class="h-1 overflow-hidden rounded-full bg-field"
					role="progressbar"
					aria-label="Download progress"
					aria-valuemin={0}
					aria-valuemax={100}
					aria-valuenow={percent ?? undefined}
				>
					<div
						class={cn('h-full rounded-full bg-accent transition-[width] duration-150', percent == null && 'w-full animate-pulse')}
						style={percent == null ? undefined : `width: ${percent}%`}
					></div>
				</div>
				<p class="mt-1.5 mb-0 font-mono text-caption text-fg-muted tabular-nums">
					{#if !phase.connected}
						<span class="font-sans">Connecting…</span>
					{:else if phase.total}
						{megabytes(phase.received)} of {megabytes(phase.total)} MB
					{:else}
						{megabytes(phase.received)} MB
					{/if}
				</p>
			</div>
		{:else}
			<div class="mt-3 flex items-center gap-1.5 pl-9.5">
				{#if phase.kind === 'available'}
					<a
						class="mr-auto text-label text-fg-muted underline-offset-2 hover:text-fg hover:underline"
						href={releaseUrl(update.version)}
						target="_blank"
						rel="noreferrer"
					>
						What’s new
					</a>
					<Button variant="ghost" size="sm" onclick={updater.dismiss}>Not now</Button>
					<Button variant="primary" size="sm" onclick={() => void updater.update()}>Update</Button>
				{:else if phase.kind === 'ready'}
					<span class="mr-auto"></span>
					<Button variant="ghost" size="sm" onclick={updater.dismiss}>Later</Button>
					<Button variant="primary" size="sm" onclick={() => void updater.restart()}>Restart now</Button>
				{:else if phase.kind === 'failed'}
					<span class="mr-auto"></span>
					<Button variant="ghost" size="sm" onclick={updater.dismiss}>Not now</Button>
					<Button variant="primary" size="sm" onclick={() => void updater.update()}>Try again</Button>
				{/if}
			</div>
		{/if}
	</section>
{/if}
