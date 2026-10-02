<script lang="ts">
	import { cn } from '#core/util'
	import type { Snippet } from 'svelte'

	interface Props {
		gap?: 'sm' | 'md'
		align?: 'start' | 'center' | 'end'
		justify?: 'start' | 'end' | 'between'
		wrap?: boolean
		children?: Snippet
	}

	let {
		gap = 'sm',
		align = 'center',
		justify = 'start',
		wrap = true,
		children,
	}: Props = $props()
</script>

<div
	class={cn(
		'flex min-w-0',
		wrap && 'flex-wrap',
		gap === 'sm' ? 'gap-2' : 'gap-3',
		align === 'start' && 'items-start',
		align === 'center' && 'items-center',
		align === 'end' && 'items-end',
		justify === 'start' && 'justify-start',
		justify === 'end' && 'justify-end',
		justify === 'between' && 'justify-between',
	)}
>
	{@render children?.()}
</div>
