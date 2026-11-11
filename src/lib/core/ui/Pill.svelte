<script lang="ts">
	import { cn } from '#core/util'
	import type { Snippet } from 'svelte'

	/** warning is production, violet is staging, info is read-only. */
	type Tone = 'neutral' | 'warning' | 'violet' | 'success' | 'danger' | 'info'

	interface Props {
		tone?: Tone
		/** Caps status badge (PROD, RO). Off: a rounded count / label chip. */
		uppercase?: boolean
		/** The 16px badge for list rows. */
		compact?: boolean
		class?: string
		children?: Snippet
	}

	let {
		tone = 'neutral',
		uppercase = false,
		compact = false,
		class: className = '',
		children,
	}: Props = $props()

	const tones: Record<Tone, string> = {
		neutral: 'bg-hover text-fg-secondary [--tint:var(--line-2)]',
		warning: 'bg-warn-soft text-warning',
		violet: 'bg-violet-soft text-violet',
		success: 'bg-accent-soft text-accent-text',
		danger: 'bg-danger-soft text-danger',
		info: 'bg-info-soft text-info',
	}
</script>

<span
	class={cn(
		uppercase
			? cn('badge [&_svg]:size-[0.6875rem]', compact && 'badge-sm')
			: 'inline-flex shrink-0 items-center rounded-full px-2 py-0.5 text-caption leading-tight font-medium ring-tint',
		tones[tone],
		className,
	)}
>
	{@render children?.()}
</span>
