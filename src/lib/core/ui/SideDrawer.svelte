<script lang="ts">
	import type { IconName } from '#icons'
	import type { Snippet } from 'svelte'
	import Icon from './Icon.svelte'
	import IconButton from './IconButton.svelte'

	/**
	 * A panel docked to the right of a canvas or grid (DBML, pending SQL):
	 * a 44px header with a glyph, title, an optional badge and actions, then
	 * the body. The close button is always last.
	 */
	interface Props {
		title: string
		icon: IconName
		/** Beside the title: a count, a state pill. */
		badge?: Snippet
		/** Icon buttons before close (copy…). */
		actions?: Snippet
		closeLabel?: string
		onclose: () => void
		children?: Snippet
	}

	let { title, icon, badge, actions, closeLabel = 'Close', onclose, children }: Props = $props()
</script>

<aside
	class="flex w-[min(24rem,38vw)] shrink-0 flex-col border-l border-line bg-surface"
	aria-label={title}
>
	<header class="flex h-11 shrink-0 items-center gap-2 border-b border-line pr-1.5 pl-3">
		<span class="flex text-fg-muted [&_svg]:size-4" aria-hidden="true"><Icon name={icon} /></span>
		<h2 class="m-0 text-label font-semibold text-fg">{title}</h2>
		{@render badge?.()}
		<span class="flex-1"></span>
		{@render actions?.()}
		<IconButton label={closeLabel} size="sm" variant="quiet" onclick={onclose}>
			<Icon name="close" />
		</IconButton>
	</header>
	<div class="drawer-body flex min-h-0 flex-1 flex-col">
		{@render children?.()}
	</div>
</aside>

<style>
	.drawer-body :global(.overflow-auto) {
		scrollbar-width: thin;
		scrollbar-color: var(--line-2) transparent;
	}
</style>
