<script lang="ts">
	import { cn } from '#core/util'
	import type { IconName } from '#icons'
	import type { Snippet } from 'svelte'
	import Icon from './Icon.svelte'

	interface Props {
		label: string
		description?: string
		htmlFor?: string
		/** Leading glyph in a 32px tile. */
		icon?: IconName
		/** Off inside a padded card body: the row brings no padding of its own. */
		inset?: boolean
		children?: Snippet
	}

	let {
		label,
		description,
		htmlFor,
		icon,
		inset = true,
		children,
	}: Props = $props()
</script>

<div
	class={cn(
		'flex items-center justify-between gap-3.5',
		inset && 'min-h-14 px-4 py-3.5',
	)}
>
	{#if icon}
		<span
			class="grid size-8 shrink-0 place-items-center rounded-[9px] bg-hover text-fg-secondary [&_svg]:size-4"
			aria-hidden="true"
		>
			<Icon name={icon} />
		</span>
	{/if}
	<div class="min-w-0 flex-1">
		{#if htmlFor}
			<label class="m-0 font-medium text-fg" for={htmlFor}>{label}</label>
		{:else}
			<p class="m-0 font-medium text-fg">{label}</p>
		{/if}
		{#if description}
			<p class="mt-0.5 mb-0 text-label text-pretty text-fg-muted">
				{description}
			</p>
		{/if}
	</div>
	<div class="shrink-0">
		{@render children?.()}
	</div>
</div>
