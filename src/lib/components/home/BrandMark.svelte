<script lang="ts">
	import { cn } from '$lib/utils/cn'

	interface Props {
		size?: 'sm' | 'md'
		class?: string
	}

	let { size = 'md', class: className = '' }: Props = $props()
</script>

<img
	src="/brand/mystique-mark.svg"
	alt=""
	width={size === 'sm' ? 28 : 32}
	height={size === 'sm' ? 28 : 32}
	aria-hidden="true"
	class={cn(
		'shrink-0 rounded-lg shadow-sm',
		size === 'sm' ? 'size-7' : 'size-8',
		className,
	)}
/>
