<script lang="ts">
	import { cn } from '#core/util'
	import { iconInner, iconViewBox, type IconName } from '#icons'

	interface Props {
		name: IconName
		class?: string
	}

	let { name, class: className = '' }: Props = $props()
</script>

<svg
	class={cn('size-3.5 shrink-0', className)}
	viewBox={iconViewBox[name]}
	fill="none"
	aria-hidden="true"
>
	{@html iconInner[name]}
</svg>
