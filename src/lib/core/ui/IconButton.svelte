<script lang="ts">
	import { cn } from '#core/util'
	import type { Snippet } from 'svelte'
	import type { HTMLButtonAttributes } from 'svelte/elements'

	/**
	 * quiet: bare glyph (toolbars, list headers). default: a neutral button.
	 * ghost: hairline border. accent: soft accent, solid on hover (the one
	 * "new" or "send" action in a toolbar).
	 */
	type Variant = 'default' | 'accent' | 'ghost' | 'quiet'
	type ControlSize = 'sm' | 'md' | 'lg'

	interface Props extends HTMLButtonAttributes {
		label: string
		variant?: Variant
		active?: boolean
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
		sm: 'size-control-sm rounded-[7px] [&_svg]:size-4',
		md: 'size-control rounded-md [&_svg]:size-4',
		lg: 'size-control-lg rounded-lg [&_svg]:size-5',
	}

	const variants: Record<Variant, string> = {
		quiet:
			'border-transparent bg-transparent text-fg-muted enabled:hover:bg-hover enabled:hover:text-fg',
		default:
			'border-line-strong bg-hover text-fg enabled:hover:bg-pressed',
		ghost:
			'border-line bg-transparent text-fg-secondary enabled:hover:bg-hover enabled:hover:text-fg',
		accent:
			'border-transparent bg-accent-soft text-accent-text enabled:hover:bg-accent enabled:hover:text-on-accent',
	}
</script>

<button
	{type}
	class={cn(
		'inline-grid shrink-0 place-items-center border p-0 leading-none',
		'transition-[color,background-color,border-color,transform] duration-150 ease-standard enabled:active:scale-[0.92]',
		sizes[size],
		active
			? 'border-transparent bg-accent-soft text-accent-text'
			: variants[variant],
		className,
	)}
	title={label}
	aria-label={label}
	aria-pressed={active}
	{...rest}
>
	{@render children?.()}
</button>
