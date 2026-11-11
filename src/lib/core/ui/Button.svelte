<script lang="ts">
	import { cn } from '#core/util'
	import type { Snippet } from 'svelte'
	import type { HTMLButtonAttributes } from 'svelte/elements'

	/**
	 * primary: the one action. secondary: neutral. ghost: quiet, no border.
	 * danger: solid, for the confirm of a destructive dialog. danger-ghost:
	 * red text, for a destructive action that is not the focus.
	 */
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
		children,
		...rest
	}: Props = $props()

	const variants: Record<Variant, string> = {
		primary:
			'border-transparent bg-accent text-on-accent shadow-[inset_0_1px_0_rgb(255_255_255/0.18),var(--shadow-1)] enabled:hover:bg-accent-hover',
		secondary:
			'border-line-strong bg-hover text-fg shadow-sm enabled:hover:bg-pressed',
		ghost:
			'border-transparent bg-transparent text-fg-secondary enabled:hover:bg-hover enabled:hover:text-fg',
		danger:
			'border-transparent bg-danger text-on-danger shadow-sm enabled:hover:bg-[color-mix(in_oklab,var(--danger)_88%,var(--text))]',
		'danger-ghost':
			'border-transparent bg-transparent text-danger enabled:hover:bg-danger-soft',
	}

	const sizes: Record<Size, string> = {
		sm: 'h-control-sm rounded-[7px] px-2.5 text-label [&_svg]:size-3.5',
		md: 'h-control rounded-md px-3 text-body [&_svg]:size-3.5',
		lg: 'h-control-lg rounded-[10px] px-4 text-body-lg [&_svg]:size-4',
	}
</script>

<button
	{type}
	class={cn(
		'inline-flex items-center justify-center gap-1.5 border leading-none font-medium whitespace-nowrap',
		'transition-[color,background-color,border-color,transform] duration-150 ease-standard enabled:active:scale-[0.97]',
		variants[variant],
		sizes[size],
		fullWidth && 'w-full',
		className,
	)}
	{...rest}
>
	{@render children?.()}
</button>
