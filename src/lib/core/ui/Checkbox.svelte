<script lang="ts">
	import { cn } from '$core/util'
	import type { Snippet } from 'svelte'
	import type { HTMLInputAttributes } from 'svelte/elements'

	interface Props extends Omit<HTMLInputAttributes, 'type'> {
		checked?: boolean
		label?: string
		children?: Snippet
	}

	let {
		checked = $bindable(false),
		label,
		class: className = '',
		children,
		...rest
	}: Props = $props()
</script>

<label
	class={cn(
		'inline-flex cursor-pointer select-none items-center gap-2 text-body text-fg-secondary',
		className,
	)}
>
	<input
		type="checkbox"
		class="m-0 size-3.5 accent-accent"
		bind:checked
		{...rest}
	/>
	{#if children}
		{@render children()}
	{:else if label}
		<span>{label}</span>
	{/if}
</label>
