<script lang="ts">
	import { cn } from '#core/util'
	import type { Snippet } from 'svelte'

	interface Props {
		/** Small caps title inside the card. */
		title?: string
		/** Form-section title in a header bar above the body (with `aside` on the right). */
		heading?: string
		aside?: string
		/** Id for the heading, so the section can be labelled by it. */
		headingId?: string
		padded?: boolean
		class?: string
		children?: Snippet
		header?: Snippet
	}

	let {
		title,
		heading,
		aside,
		headingId,
		padded = true,
		class: className = '',
		children,
		header,
	}: Props = $props()
</script>

<section
	class={cn('min-w-0 rounded-lg border border-line bg-surface', className)}
	aria-labelledby={heading ? headingId : undefined}
>
	{#if heading}
		<div class="flex items-center gap-2.5 border-b border-line px-5 py-4">
			<h2 id={headingId} class="m-0 min-w-0 flex-1 text-body-lg font-semibold">{heading}</h2>
			{#if aside}
				<span class="shrink-0 text-label text-fg-muted">{aside}</span>
			{/if}
		</div>
		<div class="flex flex-col gap-[1.125rem] p-5">
			{@render children?.()}
		</div>
	{:else}
		<div class={cn(padded && 'px-5 py-4')}>
			{#if header}
				<div class="mb-3">
					{@render header()}
				</div>
			{:else if title}
				<h2 class="m-0 mb-3 label-caps">{title}</h2>
			{/if}
			{@render children?.()}
		</div>
	{/if}
</section>
