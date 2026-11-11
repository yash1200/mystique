<script lang="ts">
	import { Icon } from '#core/ui'
	import { cn } from '#core/util'

	export interface ColorOption {
		id: string
		name: string
		hex: string
	}

	interface Props {
		colors: ColorOption[]
		value?: string
		label?: string
		onchange?: (id: string) => void
	}

	let {
		colors,
		value = $bindable(''),
		label = 'Color',
		onchange,
	}: Props = $props()

	function select(id: string) {
		value = id
		onchange?.(id)
	}
</script>

<div class="flex flex-wrap gap-2.5 px-0.5 py-1" role="group" aria-label={label}>
	{#each colors as c (c.id)}
		{@const on = value === c.id}
		<button
			type="button"
			class={cn(
				'grid size-swatch place-items-center rounded-full border-none p-0 text-white',
				'transition-[transform,box-shadow] duration-(--duration-spring) ease-spring motion-safe:hover:scale-[1.14]',
				on && 'shadow-[0_0_0_2px_var(--surface),0_0_0_4px_var(--swatch)]',
			)}
			style:--swatch={c.hex}
			style:background={c.hex}
			title={c.name}
			aria-label={c.name}
			aria-pressed={on}
			onclick={() => select(c.id)}
		>
			{#if on}
				<span class="flex animate-rise [&_svg]:size-3.5"><Icon name="check" /></span>
			{/if}
		</button>
	{/each}
</div>
