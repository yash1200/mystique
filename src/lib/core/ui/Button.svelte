<script lang="ts">
	import { cn } from '$core/util'
	import type { Snippet } from 'svelte'
	import type { HTMLButtonAttributes } from 'svelte/elements'

	type Variant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'danger-ghost'
	type Size = 'sm' | 'md' | 'lg'

	interface Props extends HTMLButtonAttributes {
		variant?: Variant
		size?: Size
		fullWidth?: boolean
		children?: Snippet
	}

	let {
		variant = 'secondary',
		size = 'md',
		fullWidth = false,
		type = 'button',
		class: className = '',
		disabled = false,
		onclick,
		children,
		...rest
	}: Props = $props()

	const variants: Record<Variant, string> = {
		primary:
			'border-transparent bg-accent text-fg-inverse hover:bg-accent-hover',
		secondary: 'border-line bg-input text-fg hover:border-line-strong',
		ghost:
			'border-line bg-transparent text-fg hover:border-line-strong hover:bg-hover',
		danger: 'border-danger/40 bg-danger/15 text-danger hover:bg-danger/25',
		'danger-ghost':
			'border-line bg-transparent text-danger hover:border-danger/50 hover:bg-danger/10',
	}

	const sizes: Record<Size, string> = {
		sm: 'h-control-sm px-2.5 text-label',
		md: 'h-control px-3.5 text-body',
		lg: 'h-control-lg px-4 text-body-lg',
	}
</script>

<button
	{type}
	class={cn(
		'inline-flex items-center justify-center gap-1.5 rounded-md border font-medium leading-none whitespace-nowrap transition-[color,background-color,border-color,transform] duration-150 ease-out enabled:active:scale-[0.97]',
		variants[variant],
		sizes[size],
		fullWidth && 'w-full',
		className,
	)}
	{disabled}
	{onclick}
	{...rest}
>
	{@render children?.()}
</button>
