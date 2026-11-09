<script lang="ts">
	import { Pill } from '#core/ui'
	import { cn } from '#core/util'
	import type { CatalogIconName } from '#features/workspace/catalog'
	import { CatalogIcon, iconToneForName } from '#features/workspace/catalog'
	import { erNodeKindShort } from '../constants'
	import type { ErNode } from '../types'
	interface Props {
		node: ErNode
		preview?: { x: number; y: number } | null
		tableW: number
		headerH: number
		rowH: number
		isSelected: boolean
		inLinkFocus: boolean
		dimmed: boolean
		isDragging: boolean
		fkColumnIds: Set<string>
		focusedColIds: Set<string> | null
		onpointerdown: (e: PointerEvent) => void
		onpointermove: (e: PointerEvent) => void
		onpointerup: (e: PointerEvent) => void
		oncontextmenu: (e: MouseEvent) => void
		oncolumnclick: (columnId: string, e: MouseEvent) => void
	}

	let {
		node,
		preview = null,
		tableW,
		headerH,
		rowH,
		isSelected,
		inLinkFocus,
		dimmed,
		isDragging,
		fkColumnIds,
		focusedColIds,
		onpointerdown,
		onpointermove,
		onpointerup,
		oncontextmenu,
		oncolumnclick,
	}: Props = $props()

	const dragDx = $derived((preview?.x ?? node.x) - node.x)
	const dragDy = $derived((preview?.y ?? node.y) - node.y)

	const iconName = $derived<CatalogIconName>(
		node.kind === 'collection'
			? 'collection'
			: node.kind === 'key_pattern'
				? 'schema'
				: 'table',
	)
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	data-er-table
	class={cn(
		'absolute overflow-hidden rounded-lg border bg-panel shadow-sm transition-opacity duration-150',
		isSelected || inLinkFocus
			? 'border-accent ring-1 ring-accent/25'
			: 'border-line hover:border-line-strong',
		dimmed && 'opacity-25',
		isDragging && 'cursor-grabbing',
	)}
	style="left: {node.x}px; top: {node.y}px; width: {tableW}px; transform: translate({dragDx}px, {dragDy}px); z-index: {isDragging
		? 5
		: inLinkFocus
			? 4
			: dimmed
				? 1
				: 2}; {isDragging ? 'will-change: transform;' : ''}"
	{onpointerdown}
	{onpointermove}
	{onpointerup}
	onpointercancel={onpointerup}
	{oncontextmenu}
>
	<div
		class={cn(
			'relative flex min-w-0 cursor-grab items-center gap-1.5 overflow-hidden border-b border-line bg-elevated px-2',
			isDragging && 'cursor-grabbing',
		)}
		style="height: {headerH}px"
	>
		<span class="absolute inset-x-0 top-0 h-0.5 bg-conn" aria-hidden="true"
		></span>
		<span
			class={cn(
				'grid size-5 shrink-0 place-items-center rounded-sm',
				isSelected || inLinkFocus ? 'bg-accent-muted' : 'bg-input',
			)}
			aria-hidden="true"
		>
			<CatalogIcon
				name={iconName}
				class="size-3"
				tone={iconToneForName(iconName, isSelected || inLinkFocus)}
			/>
		</span>
		<span
			class="min-w-0 flex-1 truncate font-mono text-label font-semibold leading-tight text-fg"
			title={node.name}
		>
			{node.name}
		</span>
		<Pill tone="neutral" uppercase>{erNodeKindShort(node.kind)}</Pill>
	</div>

	<ul class="m-0 list-none overflow-hidden p-0">
		{#each node.columns as col (col.id)}
			<li class="m-0 overflow-hidden p-0" style="height: {rowH}px">
				<button
					type="button"
					data-er-column
					class={cn(
						'box-border flex h-full w-full min-w-0 cursor-default items-center gap-1.5 overflow-hidden border-b border-line/50 px-2.5 text-left last:border-b-0',
						focusedColIds?.has(col.id) && 'bg-accent-muted',
					)}
					onclick={(e) => oncolumnclick(col.id, e)}
				>
					{#if col.pk}
						<span
							class="shrink-0 text-micro font-semibold leading-none text-accent"
							>PK</span
						>
					{:else if fkColumnIds.has(col.id)}
						<span
							class="shrink-0 text-micro font-semibold leading-none text-fg-muted"
							>FK</span
						>
					{/if}
					<span
						class="min-w-0 flex-1 truncate font-mono text-caption leading-normal text-fg"
						title={col.name}
					>
						{col.name}
					</span>
					<span
						class="max-w-[40%] shrink-0 truncate font-mono text-caption leading-normal text-fg-muted"
						title={col.dataType}>{col.dataType}</span
					>
				</button>
			</li>
		{/each}
	</ul>
</div>
