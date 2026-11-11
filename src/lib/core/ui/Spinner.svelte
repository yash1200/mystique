<script lang="ts">
	import { cn } from '#core/util'

	interface Props {
		/** xs 10px (inline with text), sm 12px, md 14px. */
		size?: 'xs' | 'sm' | 'md'
		/** Accessible name; omit when a visible label says the same. */
		label?: string
		class?: string
	}

	let { size = 'md', label, class: className = '' }: Props = $props()

	const sizes = {
		xs: 'size-2.5 border-[1.5px]',
		sm: 'size-3 border-2',
		md: 'size-3.5 border-2',
	}
</script>

<span
	class={cn(
		'inline-block shrink-0 animate-spin rounded-full border-current border-r-transparent',
		sizes[size],
		className,
	)}
	role={label ? 'status' : undefined}
	aria-label={label}
	aria-hidden={label ? undefined : 'true'}
></span>
