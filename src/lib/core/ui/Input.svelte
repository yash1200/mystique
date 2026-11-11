<script lang="ts">
	import { cn } from '#core/util'
	import type { HTMLInputAttributes } from 'svelte/elements'

	interface Props extends Omit<HTMLInputAttributes, 'size'> {
		value?: string | number | null
		/** sm 28 · md 36 · lg 40 (a dialog's search). */
		inputSize?: 'sm' | 'md' | 'lg'
		/** Extra inset for a leading / trailing icon inside the field. */
		inset?: 'none' | 'start' | 'end' | 'both'
		/** Show the field as invalid (danger border and halo). */
		invalid?: boolean
	}

	let {
		value = $bindable(''),
		inputSize = 'md',
		inset = 'none',
		invalid = false,
		class: className = '',
		type = 'text',
		spellcheck = false,
		autocomplete = 'off',
		...rest
	}: Props = $props()

	const sizes = {
		sm: 'h-control-sm rounded-md text-label',
		md: 'h-field rounded-md text-body',
		lg: 'h-control-lg rounded-[10px] text-[0.84375rem]',
	}

	/** Room for a leading 16px glyph; the 40px field sets it further in. */
	const lead = $derived(inputSize === 'lg' ? 'pl-[2.375rem]' : 'pl-8')
	const pad = $derived({
		none: 'px-3',
		start: `pr-3 ${lead}`,
		end: 'pr-8 pl-3',
		both: `pr-8 ${lead}`,
	})
</script>

<input
	class={cn(
		'w-full cursor-text border border-line-strong bg-field text-fg placeholder:text-fg-muted',
		'transition-[border-color,box-shadow] duration-150 ease-standard focus-field disabled:opacity-55',
		pad[inset],
		sizes[inputSize],
		invalid &&
			'border-danger shadow-[0_0_0_3px_var(--danger-soft)] focus-visible:border-danger focus-visible:shadow-[0_0_0_3px_var(--danger-soft)]',
		type === 'number' &&
			'[appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none',
		className,
	)}
	aria-invalid={invalid || undefined}
	{type}
	{spellcheck}
	{autocomplete}
	bind:value
	{...rest}
/>
