<script lang="ts">
	import { Input } from '$lib/components/core'
	import type { ErNode } from '$lib/types/er'
	import { cn } from '$lib/utils/cn'

	interface Props {
		nodeLabel: string
		entityWord: string
		tableQuery: string
		searchWrap: HTMLDivElement | null
		navNodes: ErNode[]
		totalCount: number
		selectedTableIds: string[]
		onfocus: (id: string) => void
	}

	let {
		nodeLabel,
		entityWord,
		tableQuery = $bindable(''),
		searchWrap = $bindable<HTMLDivElement | null>(null),
		navNodes,
		totalCount,
		selectedTableIds,
		onfocus,
	}: Props = $props()
</script>

<aside
	class="flex w-56 shrink-0 flex-col border-r border-line bg-panel"
	aria-label="Schema objects"
>
	<div class="shrink-0 border-b border-line p-2" bind:this={searchWrap}>
		<p
			class="text-label font-semibold tracking-wider text-fg-muted uppercase pb-1"
		>
			{nodeLabel}s
		</p>

		<Input
			bind:value={tableQuery}
			inputSize="sm"
			placeholder={`Find ${entityWord} or column`}
			aria-label={`Find ${entityWord} or column`}
		/>
	</div>
	<ul class="m-0 min-h-0 flex-1 list-none overflow-y-auto p-1">
		{#each navNodes as node (node.id)}
			<li class="m-0 p-0">
				<button
					type="button"
					class={cn(
						'flex w-full items-baseline justify-between gap-2 rounded-md px-2 py-1.5 text-left',
						selectedTableIds.includes(node.id)
							? 'bg-accent-muted text-fg'
							: 'text-fg-secondary hover:bg-hover',
					)}
					onclick={() => onfocus(node.id)}
				>
					<span class="min-w-0 truncate font-mono text-caption"
						>{node.name}</span
					>
					<span class="shrink-0 font-mono text-micro text-fg-muted"
						>{node.columns.length}</span
					>
				</button>
			</li>
		{:else}
			<li class="px-2 py-3 text-caption text-fg-muted">
				{tableQuery.trim()
					? `No ${entityWord} matches.`
					: `No ${entityWord}s in this database.`}
			</li>
		{/each}
	</ul>
</aside>
