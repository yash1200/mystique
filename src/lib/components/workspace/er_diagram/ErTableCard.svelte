<script lang="ts">
	import { Button, Checkbox, Icon, IconButton, Input, Pill, Select } from '$lib/components/core'
	import type { DataFamily, ErNode } from '$lib/types/er'
	import { defaultColumnType } from '$lib/types/er'
	import { cn } from '$lib/utils/cn'
	import type { CatalogIconName } from '../catalogIcon'
	import { iconToneForName } from '../catalogIcon'
	import CatalogIcon from '../CatalogIcon.svelte'
	import { erNodeKindShort } from './erConstants'

	interface Props {
		node: ErNode
		/** Unsnapped live position while this table is being dragged. */
		preview?: { x: number; y: number } | null
		tableW: number
		headerH: number
		rowH: number
		isSelected: boolean
		inLinkFocus: boolean
		dimmed: boolean
		readOnly: boolean
		arrowTool: 'normal' | 'broken' | null
		isPrimary: boolean
		isDragging: boolean
		fkColumnIds: Set<string>
		focusedColIds: Set<string> | null
		linkFrom: { tableId: string; columnId: string } | null
		typeOptions: { value: string; label: string }[]
		family: DataFamily
		showAddCol: boolean
		newColName: string
		newColType: string
		newColPk: boolean
		onpointerdown: (e: PointerEvent) => void
		onpointermove: (e: PointerEvent) => void
		onpointerup: (e: PointerEvent) => void
		oncontextmenu: (e: MouseEvent) => void
		oncolumnclick: (columnId: string, e: MouseEvent) => void
		onrename: (name: string) => void
		onupdatetype: (colId: string, dataType: string) => void
		ondeletecolumn: (colId: string) => void
		onaddcolumn: () => void
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
		readOnly,
		arrowTool,
		isPrimary,
		isDragging,
		fkColumnIds,
		focusedColIds,
		linkFrom,
		typeOptions,
		family,
		showAddCol = $bindable(false),
		newColName = $bindable(''),
		newColType = $bindable(''),
		newColPk = $bindable(false),
		onpointerdown,
		onpointermove,
		onpointerup,
		oncontextmenu,
		oncolumnclick,
		onrename,
		onupdatetype,
		ondeletecolumn,
		onaddcolumn,
	}: Props = $props()

	const editing = $derived(!readOnly && isPrimary && !arrowTool)
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
		arrowTool ? 'cursor-default' : '',
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
			'relative flex min-w-0 items-center gap-1.5 overflow-hidden border-b border-line bg-elevated px-2',
			!arrowTool && 'cursor-grab',
			isDragging && 'cursor-grabbing',
		)}
		style="height: {headerH}px"
	>
		<span
			class="absolute inset-x-0 top-0 h-0.5 bg-conn"
			aria-hidden="true"
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
		{#if editing}
			<Input
				class="min-w-0 flex-1 font-mono text-label font-semibold"
				value={node.name}
				inputSize="sm"
				data-no-drag
				aria-label="Table name"
				oninput={(e) => onrename((e.currentTarget as HTMLInputElement).value)}
				onclick={(e) => e.stopPropagation()}
			/>
		{:else}
			<span
				class="min-w-0 flex-1 truncate font-mono text-label font-semibold leading-tight text-fg"
				title={node.name}
			>
				{node.name}
			</span>
		{/if}
		<Pill tone="neutral" uppercase>{erNodeKindShort(node.kind)}</Pill>
	</div>

	<ul class="m-0 list-none overflow-hidden p-0">
		{#each node.columns as col (col.id)}
			<li class="m-0 overflow-hidden p-0" style="height: {rowH}px">
				{#if editing}
					<div
						data-er-column
						data-no-drag
						class={cn(
							'box-border flex h-full w-full min-w-0 items-center gap-1 overflow-hidden border-b border-line/50 px-1.5 last:border-b-0',
							focusedColIds?.has(col.id) && 'bg-accent-muted',
						)}
					>
						{#if col.pk}
							<span class="shrink-0 text-micro font-semibold leading-none text-accent"
								>PK</span
							>
						{:else if fkColumnIds.has(col.id)}
							<span
								class="shrink-0 text-micro font-semibold leading-none text-fg-muted"
								>FK</span
							>
						{/if}
						<span
							class="min-w-0 flex-1 truncate font-mono text-caption leading-none text-fg"
							title={col.name}
						>
							{col.name}
						</span>
						<div
							class="w-[min(7.5rem,38%)] max-w-[42%] shrink-0 cursor-pointer [&_button]:box-border [&_button]:h-control-sm [&_button]:max-h-full [&_button]:px-1.5 [&_button]:text-caption [&_svg]:size-3"
							role="presentation"
							onclick={(e) => e.stopPropagation()}
							onpointerdown={(e) => e.stopPropagation()}
						>
							<Select
								value={col.dataType}
								options={typeOptions.some((o) => o.value === col.dataType)
									? typeOptions
									: [{ value: col.dataType, label: col.dataType }, ...typeOptions]}
								inputSize="sm"
								onchange={(v) => onupdatetype(col.id, v)}
							/>
						</div>
						<IconButton
							label="Delete column"
							size="sm"
							variant="quiet"
							class="text-fg-muted hover:bg-danger/10 hover:text-danger"
							onclick={(e: MouseEvent) => {
								e.stopPropagation()
								ondeletecolumn(col.id)
							}}
						>
							<Icon name="close" class="size-3" />
						</IconButton>
					</div>
				{:else}
					<button
						type="button"
						data-er-column
						data-no-drag
						class={cn(
							'box-border flex h-full w-full min-w-0 cursor-default items-center gap-1.5 overflow-hidden border-b border-line/50 px-2.5 text-left last:border-b-0',
							arrowTool && 'cursor-pointer hover:bg-accent-muted',
							linkFrom?.tableId === node.id &&
								linkFrom?.columnId === col.id &&
								'bg-accent-muted ring-1 ring-inset ring-accent',
							focusedColIds?.has(col.id) && 'bg-accent-muted',
						)}
						onclick={(e) => oncolumnclick(col.id, e)}
					>
						{#if col.pk}
							<span class="shrink-0 text-micro font-semibold leading-none text-accent"
								>PK</span
							>
						{:else if fkColumnIds.has(col.id)}
							<span
								class="shrink-0 text-micro font-semibold leading-none text-fg-muted"
								>FK</span
							>
						{/if}
						<span
							class="min-w-0 flex-1 truncate font-mono text-caption leading-none text-fg"
							title={col.name}
						>
							{col.name}
						</span>
						<span
							class="max-w-[40%] shrink-0 truncate font-mono text-caption leading-none text-fg-muted"
							title={col.dataType}>{col.dataType}</span
						>
					</button>
				{/if}
			</li>
		{/each}
	</ul>

	{#if editing}
		<div class="cursor-default border-t border-line p-2" data-no-drag>
			{#if !showAddCol}
				<Button
					variant="ghost"
					size="sm"
					fullWidth
					onclick={(e: MouseEvent) => {
						e.stopPropagation()
						newColType = defaultColumnType(family)
						showAddCol = true
					}}
				>
					<Icon name="plus" class="size-3.5" />
					Add column
				</Button>
			{:else}
				<div class="flex flex-col gap-2">
					<Input
						bind:value={newColName}
						placeholder="column_name"
						inputSize="sm"
						class="cursor-text font-mono"
					/>
					<div
						class="cursor-pointer"
						role="presentation"
						onclick={(e) => e.stopPropagation()}
						onpointerdown={(e) => e.stopPropagation()}
					>
						<Select
							bind:value={newColType}
							options={typeOptions}
							inputSize="sm"
							placeholder="Column type"
						/>
					</div>
					<Checkbox bind:checked={newColPk} label="Primary key" class="text-label" />
					<div class="flex gap-1.5">
						<Button
							size="sm"
							variant="primary"
							onclick={(e: MouseEvent) => {
								e.stopPropagation()
								onaddcolumn()
							}}
						>
							Add
						</Button>
						<Button
							size="sm"
							variant="ghost"
							onclick={(e: MouseEvent) => {
								e.stopPropagation()
								showAddCol = false
							}}
						>
							Cancel
						</Button>
					</div>
				</div>
			{/if}
		</div>
	{/if}
</div>
