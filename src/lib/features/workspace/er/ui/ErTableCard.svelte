<script lang="ts">
	import { Icon, Pill } from '#core/ui'
	import { cn, splitQualifiedName } from '#core/util'
	import type { IconName } from '#icons'
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
	const lit = $derived(isSelected || inLinkFocus)

	const icon = $derived<IconName>(
		node.kind === 'collection' ? 'collection' : node.kind === 'key_pattern' ? 'schema' : 'table',
	)

	const title = $derived(splitQualifiedName(node.name))

	const z = $derived(isDragging ? 5 : inLinkFocus ? 4 : dimmed ? 1 : 2)
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	data-er-table
	class={cn(
		'absolute overflow-hidden rounded-xl border bg-surface shadow-md',
		'transition-[opacity,border-color,box-shadow] duration-150',
		lit
			? 'border-accent shadow-[0_0_0_1px_var(--accent),var(--shadow-2)]'
			: 'border-line hover:border-line-strong',
		dimmed && 'opacity-30',
		isDragging && 'cursor-grabbing shadow-lg',
	)}
	style="left: {node.x}px; top: {node.y}px; width: {tableW}px; transform: translate({dragDx}px, {dragDy}px); z-index: {z};{isDragging
		? ' will-change: transform;'
		: ''}"
	{onpointerdown}
	{onpointermove}
	{onpointerup}
	onpointercancel={onpointerup}
	{oncontextmenu}
>
	<div
		class={cn(
			'flex min-w-0 cursor-grab items-center gap-2 border-b border-line pr-2 pl-3',
			isDragging && 'cursor-grabbing',
		)}
		style="height: {headerH}px"
	>
		<span
			class={cn('flex shrink-0 [&_svg]:size-3.5', lit ? 'text-accent-text' : 'text-fg-muted')}
			aria-hidden="true"
		>
			<Icon name={icon} />
		</span>
		<span class="min-w-0 flex-1 truncate font-mono text-label leading-tight" title={node.name}>
			{#if title.schema}<span class="text-fg-muted">{title.schema}</span>{/if}<span
				class="font-semibold text-fg">{title.name}</span
			>
		</span>
		<Pill uppercase compact>{erNodeKindShort(node.kind)}</Pill>
	</div>

	<ul class="m-0 list-none p-0 [&>li:last-child>button]:border-b-0">
		{#each node.columns as col (col.id)}
			{@const fk = !col.pk && fkColumnIds.has(col.id)}
			<li class="m-0 p-0" style="height: {rowH}px">
				<button
					type="button"
					data-er-column
					class={cn(
						'box-border flex h-full w-full min-w-0 cursor-default items-center gap-2 border-b border-line/60 pr-3 pl-3 text-left',
						'transition-colors duration-150',
						focusedColIds?.has(col.id) ? 'bg-accent-soft' : 'hover:bg-hover',
					)}
					onclick={(e) => oncolumnclick(col.id, e)}
				>
					<span
						class={cn(
							'w-5 shrink-0 font-mono text-[0.625rem] font-semibold tracking-wide',
							col.pk ? 'text-accent-text' : 'text-info',
						)}
					>
						{col.pk ? 'PK' : fk ? 'FK' : ''}
					</span>
					<span class="min-w-0 flex-1 truncate font-mono text-caption text-fg" title={col.name}>
						{col.name}
					</span>
					<span
						class="max-w-[45%] shrink-0 truncate font-mono text-caption text-fg-muted"
						title={col.dataType}
					>
						{col.dataType}
					</span>
				</button>
			</li>
		{/each}
	</ul>
</div>
