<script lang="ts">
	import { cn } from '$core/util'
	import type { Snippet } from 'svelte'

	interface Props {
		color: string
		tinted?: boolean
		selected?: boolean
		class?: string
		children?: Snippet
	}

	let {
		color,
		tinted = false,
		selected = false,
		class: className = '',
		children,
	}: Props = $props()
</script>

<div
	class={cn(
		'flex min-w-0 items-stretch gap-2',
		// Parent list item owns the selected background tint to avoid double layers.
		tinted && !selected && 'conn-tint',
		className,
	)}
	style="--connection-color: {color}"
>
	<span
		class={cn(
			'w-stripe shrink-0 rounded-r-sm bg-conn',
			selected || tinted ? 'opacity-100' : 'opacity-85',
		)}
		aria-hidden="true"
	></span>
	<div class="min-w-0 flex-1">
		{@render children?.()}
	</div>
</div>
