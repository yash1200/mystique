<script lang="ts">
	import { cn } from '#core/util'
	import type { ConnectionFact } from '../facts'

	interface Props {
		facts: ConnectionFact[]
		/** One column below the `sm` breakpoint (the home detail panel). */
		stack?: boolean
	}

	let { facts, stack = false }: Props = $props()
</script>

<div
	class={cn(
		'grid gap-px overflow-hidden rounded-lg border border-line bg-line',
		stack ? 'grid-cols-1 sm:grid-cols-2' : 'grid-cols-2',
	)}
>
	{#each facts as item (item.label)}
		<div
			class={cn(
				'bg-elevated px-3 py-2.5',
				item.wide && (stack ? 'sm:col-span-2' : 'col-span-2'),
			)}
		>
			<div class="text-micro font-semibold tracking-wider text-fg-muted uppercase">
				{item.label}
			</div>
			<div
				class="mt-1 min-w-0 truncate font-mono text-label text-fg"
				title={item.value}
			>
				{item.value}
			</div>
		</div>
	{/each}
</div>
