<script lang="ts">
	import { cn } from '#core/util'
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
			'w-full rounded-md border border-line-strong bg-field pl-3 font-mono text-fg placeholder:font-sans placeholder:text-fg-muted',
			'transition-[border-color,box-shadow] duration-150 ease-standard focus-field disabled:opacity-55',
			hasValue ? 'pr-16' : 'pr-3',
			inputSize === 'sm' ? 'h-control-sm text-label' : 'h-field text-body',
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
				'absolute top-1/2 right-1 h-control-sm -translate-y-1/2 rounded-[7px] border-none bg-transparent px-2.5',
				'text-label font-medium text-fg-secondary transition-colors duration-150',
				'hover:bg-hover hover:text-fg disabled:pointer-events-none disabled:opacity-55',
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
