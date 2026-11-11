<script lang="ts">
	import { cn } from '#core/util'
	import type { Snippet } from 'svelte'

	interface Props {
		left?: Snippet
		center?: Snippet
		right?: Snippet
		class?: string
		ariaLabel?: string
	}

	let {
		left,
		center,
		right,
		class: className = '',
		ariaLabel = 'Status',
	}: Props = $props()
</script>

<footer
	class={cn(
		'flex h-statusbar shrink-0 items-center gap-x-2.5 border-t border-line bg-side pr-2 pl-3.5',
		className,
	)}
	aria-label={ariaLabel}
>
	<div
		class="flex h-full min-w-0 flex-1 items-center gap-2.5 self-stretch overflow-hidden text-label leading-none text-fg-muted"
	>
		{@render left?.()}
	</div>
	{#if center}
		<div
			class="flex h-full shrink-0 items-center gap-1.5 self-stretch leading-none"
		>
			{@render center()}
		</div>
	{/if}
	{#if right}
		<div
			class="flex h-full min-w-0 flex-1 items-center justify-end gap-1 self-stretch text-label leading-none text-fg-muted"
		>
			{@render right()}
		</div>
	{:else if center}
		<!-- Balance center when there is no right slot -->
		<div class="min-w-0 flex-1" aria-hidden="true"></div>
	{/if}
</footer>
