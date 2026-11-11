<script lang="ts">
	import { cn } from '#core/util'
	import type { Snippet } from 'svelte'

	/** A mono mark (engine, type). For status words use `Pill`. */
	type Tone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger' | 'info'

	interface Props {
		tone?: Tone
		size?: 'sm' | 'md'
		children?: Snippet
	}

	let { tone = 'neutral', size = 'sm', children }: Props = $props()

	const tones: Record<Tone, string> = {
		neutral: 'bg-hover text-fg-secondary [--tint:var(--line-2)]',
		accent: 'bg-accent-soft text-accent-text',
		success: 'bg-accent-soft text-accent-text',
		warning: 'bg-warn-soft text-warning',
		danger: 'bg-danger-soft text-danger',
		info: 'bg-info-soft text-info',
	}
</script>

<span
	class={cn(
		'inline-flex shrink-0 items-center justify-center gap-1 rounded-xs font-mono leading-none font-semibold ring-tint',
		size === 'sm'
			? 'h-5 min-w-7 px-1 text-micro'
			: 'h-7 min-w-9 px-1.5 text-caption',
		tones[tone],
	)}
>
	{@render children?.()}
</span>
