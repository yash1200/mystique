<script lang="ts">
	import { cn } from '#core/util'
	import type { Snippet } from 'svelte'

	interface Props {
		title?: string
		padded?: boolean
		class?: string
		children?: Snippet
		header?: Snippet
	}

	let {
		title,
		padded = true,
		class: className = '',
		children,
		header,
	}: Props = $props()
</script>

<section
	class={cn(
		'min-w-0 rounded-xl border border-line bg-elevated shadow-sm',
		padded && 'px-5 py-4',
		className,
	)}
>
	{#if header}
		<div class="mb-3">
			{@render header()}
		</div>
	{:else if title}
		<h2
			class="mb-3 text-label font-semibold tracking-wider text-fg-muted uppercase"
		>
			{title}
		</h2>
	{/if}
	{@render children?.()}
</section>
