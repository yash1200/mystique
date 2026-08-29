<script lang="ts">
	import { cn } from '$core/util'
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

<div class="flex flex-wrap gap-2" role="group" aria-label={label}>
	{#each colors as c (c.id)}
		<button
			type="button"
			class={cn(
				'size-swatch rounded-full border-2 border-transparent p-0 motion-safe:hover:scale-105',
				value === c.id &&
					'shadow-[0_0_0_2px_var(--bg-app),0_0_0_4px_var(--border-focus)]',
			)}
			style="background: {c.hex}"
			title={c.name}
			aria-label={c.name}
			aria-pressed={value === c.id}
			onclick={() => select(c.id)}
		></button>
	{/each}
</div>
