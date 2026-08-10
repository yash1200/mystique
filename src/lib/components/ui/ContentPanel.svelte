<script lang="ts">
	import { cn } from '$lib/utils/cn'
	import type { Snippet } from 'svelte'

	type PanelSize = 'sm' | 'md' | 'lg' | 'full'

	interface Props {
		/** Prefer size tokens over raw maxWidth numbers. */
		size?: PanelSize
		maxWidth?: number | string
		/** Vertically center in the parent scroll area when content is shorter. */
		center?: boolean
		ariaLabel?: string
		class?: string
		children?: Snippet
	}

	let {
		size = 'md',
		maxWidth,
		center = false,
		ariaLabel,
		class: className = '',
		children,
	}: Props = $props()

	const sizeClass: Record<PanelSize, string> = {
		sm: 'max-w-panel-sm',
		md: 'max-w-panel-md',
		lg: 'max-w-panel-lg',
		full: 'max-w-none',
	}

	const maxStyle = $derived(
		maxWidth == null
			? undefined
			: typeof maxWidth === 'number'
				? `${maxWidth}px`
				: maxWidth,
	)
</script>

<section
	class={cn(
		'mx-auto w-full px-6 py-8',
		!maxWidth && sizeClass[size],
		center && 'flex min-h-full items-center',
		className,
	)}
	style={maxStyle ? `max-width: ${maxStyle}` : undefined}
	aria-label={ariaLabel}
>
	<div class={center ? 'w-full' : undefined}>
		{@render children?.()}
	</div>
</section>
