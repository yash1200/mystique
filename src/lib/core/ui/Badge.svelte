<script lang="ts">
	import { cn } from '#core/util'
	import type { Snippet } from 'svelte'

	type Tone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger' | 'info'

	interface Props {
		tone?: Tone
		size?: 'sm' | 'md'
		children?: Snippet
	}

	let { tone = 'neutral', size = 'sm', children }: Props = $props()

	const tones: Record<Tone, string> = {
		neutral: 'border-line bg-input text-fg-secondary',
		accent: 'border-accent/35 bg-accent-muted text-accent',
		success: 'border-success/35 bg-success/15 text-success',
		warning: 'border-warning/45 bg-warning/15 text-warning',
		danger: 'border-danger/40 bg-danger/15 text-danger',
		info: 'border-info/35 bg-info/15 text-info',
	}
</script>

<span
	class={cn(
		'inline-flex shrink-0 items-center justify-center gap-1 rounded-sm border font-mono font-semibold leading-none tracking-tight',
		size === 'sm'
			? 'h-5 min-w-7 px-1 text-micro'
			: 'h-7 min-w-9 px-1.5 text-caption',
		tones[tone],
	)}
>
	{@render children?.()}
</span>
