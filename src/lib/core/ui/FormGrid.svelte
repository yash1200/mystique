<script lang="ts">
	import { cn } from '$core/util'
	import type { Snippet } from 'svelte'

	interface Props {
		columns?: 1 | 2 | 3
		gap?: 'sm' | 'md'
		children?: Snippet
	}

	let { columns = 2, gap = 'md', children }: Props = $props()
</script>

<div
	class={cn(
		'grid min-w-0',
		columns === 1 && 'grid-cols-1',
		columns === 2 && 'grid-cols-1 sm:grid-cols-2',
		columns === 3 && 'grid-cols-1 sm:grid-cols-3',
		gap === 'sm' ? 'gap-2' : 'gap-3',
	)}
>
	{@render children?.()}
</div>
