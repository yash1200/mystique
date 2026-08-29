<script lang="ts">
	import { cn } from '$core/util'
	import type { Snippet } from 'svelte'
	import type { HTMLButtonAttributes } from 'svelte/elements'

	type Variant = 'default' | 'accent' | 'ghost' | 'quiet'
	/** Matches Input/Button control scale from the design language. */
	type ControlSize = 'sm' | 'md' | 'lg'

	interface Props extends HTMLButtonAttributes {
		label: string
		variant?: Variant
		active?: boolean
		/** Design-language size; aligns with Input `inputSize`. */
		size?: ControlSize
		children?: Snippet
	}

	let {
		label,
		variant = 'default',
		active = false,
		size = 'md',
		type = 'button',
		class: className = '',
		children,
		...rest
	}: Props = $props()

	const sizes: Record<ControlSize, string> = {
		sm: 'size-control-sm text-label [&_svg]:size-4',
		md: 'size-control text-body [&_svg]:size-5',
		lg: 'size-control-lg text-body-lg [&_svg]:size-5',
	}
</script>

<button
	{type}
	class={cn(
		'inline-grid shrink-0 place-items-center rounded-md border p-0 leading-none transition-[color,background-color,border-color,transform] duration-150 ease-out enabled:active:scale-[0.97]',
		sizes[size],
		variant === 'ghost' && 'border-line bg-transparent text-fg',
		variant === 'quiet' &&
			'border-transparent bg-transparent text-fg-secondary',
		variant === 'default' && 'border-line bg-elevated text-fg',
		variant === 'accent' &&
			!active &&
			'border-transparent bg-accent text-fg-inverse hover:bg-accent-hover',
		variant === 'quiet' && !active && 'hover:bg-hover hover:text-fg',
		variant !== 'accent' &&
			variant !== 'quiet' &&
			!active &&
			'hover:border-line-strong hover:bg-hover',
		active && 'border-accent bg-accent-muted text-accent',
		className,
	)}
	title={label}
	aria-label={label}
	aria-pressed={active}
	{...rest}
>
	{@render children?.()}
</button>
