<script lang="ts">
	import { Button, Icon } from '#core/ui'
	import type { IconName } from '#icons'

	interface Props {
		icon: IconName
		title: string
		description: string
		accent?: boolean
		actionLabel?: string
		onaction?: () => void
	}

	let {
		icon,
		title,
		description,
		accent = false,
		actionLabel,
		onaction,
	}: Props = $props()
</script>

<div class="m-auto flex max-w-panel-sm flex-col items-center py-8 text-center">
	<span
		class={accent
			? 'grid size-10 place-items-center rounded-xl bg-accent-muted text-accent'
			: 'grid size-10 place-items-center rounded-xl bg-elevated text-fg-muted ring-1 ring-line'}
		aria-hidden="true"
	>
		<Icon name={icon} class="size-5" />
	</span>
	<p class="mt-3 mb-0 text-body font-semibold text-fg">{title}</p>
	<p
		class="mt-1.5 mb-0 max-w-prose-ui text-caption leading-relaxed text-fg-muted"
		class:mb-3={Boolean(actionLabel)}
	>
		{description}
	</p>
	{#if actionLabel && onaction}
		<Button size="sm" onclick={onaction}>{actionLabel}</Button>
	{/if}
</div>
