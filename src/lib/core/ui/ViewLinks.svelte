<script lang="ts">
	import type { IconName } from '#icons'
	import Icon from './Icon.svelte'

	export type ViewLink = {
		label: string
		icon: IconName
		title?: string
		onclick: () => void
	}

	/**
	 * The other views of the same object (Data, Structure, ERD), at the
	 * start of a status bar. A hairline closes the group off from the status.
	 */
	interface Props {
		links: ViewLink[]
		label?: string
	}

	let { links, label = 'Views' }: Props = $props()
</script>

{#if links.length}
	<nav class="flex shrink-0 items-center gap-0.5" aria-label={label}>
		{#each links as link (link.label)}
			<button
				type="button"
				class="group inline-flex h-6.5 items-center gap-1.5 rounded-[7px] border-none bg-transparent px-2 text-label font-medium text-fg-secondary transition-colors duration-150 hover:bg-hover hover:text-fg"
				title={link.title}
				onclick={link.onclick}
			>
				<span
					class="flex text-fg-muted transition-colors duration-150 group-hover:text-accent-text [&_svg]:size-3.5"
					aria-hidden="true"
				>
					<Icon name={link.icon} />
				</span>
				{link.label}
			</button>
		{/each}
		<span class="mx-1.5 h-4 w-px bg-line" aria-hidden="true"></span>
	</nav>
{/if}
