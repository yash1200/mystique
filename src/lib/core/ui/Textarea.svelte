<script lang="ts">
	import { cn } from '#core/util'
	import type { HTMLTextareaAttributes } from 'svelte/elements'

	interface Props extends HTMLTextareaAttributes {
		value?: string
		resize?: 'none' | 'y' | 'both'
		/** Draw the standalone field chrome. Off when nested in another surface. */
		chrome?: boolean
	}

	let {
		value = $bindable(''),
		class: className = '',
		rows = 3,
		resize = 'y',
		chrome = true,
		...rest
	}: Props = $props()
</script>

<textarea
	class={cn(
		'min-h-14 w-full px-3 py-2.5 text-body text-fg',
		chrome &&
			'rounded-md border border-line-strong bg-field transition-[border-color,box-shadow] duration-150 ease-standard focus-field',
		!chrome && 'border-0 bg-transparent',
		resize === 'none' && 'resize-none',
		resize === 'y' && 'resize-y',
		resize === 'both' && 'resize',
		'placeholder:text-fg-muted focus:outline-none',
		className,
	)}
	{rows}
	bind:value
	{...rest}
></textarea>
