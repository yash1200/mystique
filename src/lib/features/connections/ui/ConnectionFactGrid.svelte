<script lang="ts">
	import { cn } from '#core/util'
	import type { ConnectionFact } from '../facts'

	interface Props {
		facts: ConnectionFact[]
		/**
		 * pairs: two columns, `wide` facts span the row (dialogs).
		 * fluid: as many 200px columns as fit (the home detail).
		 */
		layout?: 'pairs' | 'fluid'
	}

	let { facts, layout = 'pairs' }: Props = $props()
</script>

<dl
	class={cn(
		'm-0 grid overflow-hidden rounded-lg border border-line bg-surface',
		layout === 'fluid'
			? 'grid-cols-[repeat(auto-fit,minmax(min(12.5rem,100%),1fr))]'
			: 'grid-cols-2',
	)}
>
	{#each facts as item (item.label)}
		<div
			class={cn(
				'flex min-w-0 flex-col gap-1 shadow-[1px_0_0_var(--line),0_1px_0_var(--line)]',
				layout === 'fluid' ? 'px-4 py-3.5' : 'px-3 py-2.5',
				layout === 'pairs' && item.wide && 'col-span-2',
			)}
		>
			<dt class="label-caps">{item.label}</dt>
			<dd class="m-0 flex min-w-0 items-baseline gap-2">
				<span
					class={cn(
						'min-w-0 truncate text-fg',
						item.mono === false ? 'text-body' : 'font-mono text-data',
					)}
					title={item.value}
				>
					{item.value}
				</span>
				{#if item.warn}
					<span class="shrink-0 text-label text-warning">{item.warn}</span>
				{/if}
			</dd>
		</div>
	{/each}
</dl>
