<script lang="ts">
	import type { IconName } from '#icons'
	import type { Snippet } from 'svelte'
	import { cn } from '#core/util'
	import Icon from './Icon.svelte'

	/**
	 * The quiet "nothing here" inside a panel: a 32px glyph tile, a title and
	 * a line of help. `EmptyState` is the page-sized one.
	 */
	interface Props {
		title: string
		description?: string
		icon?: IconName
		/** A custom glyph in place of `icon` (a catalog icon, a spinner). */
		glyph?: Snippet
		/** Grow to fill a flex column (a whole panel body). */
		fill?: boolean
		children?: Snippet
	}

	let { title, description, icon, glyph, fill = false, children }: Props = $props()
</script>

<div
	class={cn(
		'flex flex-col items-center justify-center px-6 py-8 text-center',
		fill && 'flex-1',
	)}
>
	{#if glyph || icon}
		<span
			class="mb-2 grid size-8 place-items-center rounded-lg bg-field text-fg-muted [&_svg]:size-4"
			aria-hidden="true"
		>
			{#if glyph}{@render glyph()}{:else if icon}<Icon name={icon} />{/if}
		</span>
	{/if}
	<p class="m-0 text-label font-medium text-fg">{title}</p>
	{#if description}
		<p class="mt-1 mb-0 max-w-prose-ui text-caption leading-relaxed text-fg-muted">{description}</p>
	{/if}
	{#if children}
		<div class="mt-3">{@render children()}</div>
	{/if}
</div>
