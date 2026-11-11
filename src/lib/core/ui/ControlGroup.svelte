<script lang="ts">
	import { cn } from '#core/util'
	import type { Snippet } from 'svelte'

	/** Rows in one bordered card, divided by hairlines. */
	interface Props {
		labelledby?: string
		/** Spacing around the card (margins only: the look is fixed). */
		class?: string
		children?: Snippet
	}

	let { labelledby, class: className = '', children }: Props = $props()
</script>

<div
	class={cn('divide-y divide-line overflow-hidden rounded-lg border border-line bg-surface', className)}
	role="group"
	aria-labelledby={labelledby}
>
	{@render children?.()}
</div>
