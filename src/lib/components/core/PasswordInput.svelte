<script lang="ts">
	import { cn } from '$lib/utils/cn'
	import type { HTMLInputAttributes } from 'svelte/elements'

	interface Props extends Omit<HTMLInputAttributes, 'size' | 'type'> {
		value?: string | number | null
		inputSize?: 'sm' | 'md'
	}

	let {
		value = $bindable(''),
		inputSize = 'md',
		class: className = '',
		disabled = false,
		...rest
	}: Props = $props()

	let wantVisible = $state(false)

	const hasValue = $derived(String(value ?? '').length > 0)
	const visible = $derived(wantVisible && hasValue)
	const inputType = $derived(visible ? 'text' : 'password')
	const toggleLabel = $derived(visible ? 'Hide password' : 'Show password')
</script>

<div class={cn('relative w-full', className)}>
	<input
		class={cn(
			'w-full rounded-md border border-line bg-input pl-2.5 text-fg placeholder:text-fg-muted',
			'focus:outline-none focus-visible:border-focus focus-visible:ring-2 focus-visible:ring-accent-muted',
			'disabled:opacity-55',
			hasValue ? 'pr-16' : 'pr-2.5',
			inputSize === 'sm' ? 'h-control-sm text-label' : 'h-control text-body',
		)}
		type={inputType}
		bind:value
		{disabled}
		spellcheck="false"
		{...rest}
	/>
	{#if hasValue}
		<button
			type="button"
			class={cn(
				'absolute top-1/2 right-1.5 -translate-y-1/2 rounded-sm border-none bg-transparent',
				'px-1.5 py-0.5 text-caption font-medium text-accent',
				'hover:bg-hover hover:underline',
				'focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-focus',
				'disabled:pointer-events-none disabled:opacity-55',
			)}
			aria-label={toggleLabel}
			title={toggleLabel}
			aria-pressed={visible}
			{disabled}
			onclick={() => {
				wantVisible = !wantVisible
			}}
		>
			{visible ? 'Hide' : 'Show'}
		</button>
	{/if}
</div>
