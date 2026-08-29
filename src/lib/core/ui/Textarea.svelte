<script lang="ts">
	import { cn } from '$core/util'
	import type { HTMLTextareaAttributes } from 'svelte/elements'

	interface Props extends HTMLTextareaAttributes {
		value?: string
		resize?: 'none' | 'y' | 'both'
	}

	let {
		value = $bindable(''),
		class: className = '',
		rows = 3,
		resize = 'y',
		...rest
	}: Props = $props()
</script>

<textarea
	class={cn(
		'min-h-14 w-full rounded-md border border-line bg-input px-2.5 py-2 text-body text-fg',
		resize === 'none' && 'resize-none',
		resize === 'y' && 'resize-y',
		resize === 'both' && 'resize',
		'placeholder:text-fg-muted focus:outline-none focus-visible:border-focus focus-visible:ring-2 focus-visible:ring-accent-muted',
		className,
	)}
	{rows}
	bind:value
	{...rest}
></textarea>
