<script lang="ts">
	import { Button, Input, Select } from '$lib/components/core'
	import type { DataFamily, ErNode } from '$lib/types/er'
	import { defaultColumnType } from '$lib/types/er'
	import { cn } from '$lib/utils/cn'
	import { erNodeKindShort } from './erConstants'

	interface Props {
		node: ErNode
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
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	data-er-table
	class={cn(
		'absolute overflow-hidden rounded-lg border border-l-2 bg-panel shadow-sm transition-opacity duration-150',
		isSelected || inLinkFocus
			? 'border-accent ring-2 ring-accent/30'
			: 'border-line hover:border-line-strong',
		dimmed && 'opacity-25',
		arrowTool ? 'cursor-default' : '',
		isDragging && 'cursor-grabbing',
	)}
	style="left: {node.x}px; top: {node.y}px; width: {tableW}px; z-index: {inLinkFocus
		? 4
		: dimmed
			? 1
			: 2}; border-left-color: var(--connection-color, var(--accent));"
	{onpointerdown}
	{onpointermove}
	{onpointerup}
	onpointercancel={onpointerup}
	{oncontextmenu}
>
	<div
		class={cn(
			'flex min-w-0 items-center justify-between gap-2 overflow-hidden border-b border-line bg-elevated px-2.5',
			!arrowTool && 'cursor-grab',
			isDragging && 'cursor-grabbing',
		)}
		style="height: {headerH}px"
	>
		{#if editing}
			<input
				class="box-border h-control-sm min-w-0 max-w-full flex-1 cursor-text truncate rounded-md border border-line bg-input px-2 text-body font-semibold leading-none text-fg"
				value={node.name}
				data-no-drag
				oninput={(e) => onrename((e.currentTarget as HTMLInputElement).value)}
				onclick={(e) => e.stopPropagation()}
			/>
		{:else}
			<span class="min-w-0 flex-1 truncate text-body font-semibold leading-tight text-fg"
				>{node.name}</span
			>
		{/if}
		<span
			class="shrink-0 text-micro font-semibold tracking-wider text-fg-muted uppercase"
		>
			{erNodeKindShort(node.kind)}
		</span>
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
						<button
							type="button"
							class="flex size-control-sm shrink-0 cursor-pointer items-center justify-center rounded text-body leading-none text-danger hover:bg-danger/10"
							onclick={(e) => {
								e.stopPropagation()
								ondeletecolumn(col.id)
							}}
							aria-label="Delete column"
						>
							×
						</button>
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
		<div class="cursor-default border-t border-line p-2.5" data-no-drag>
			{#if !showAddCol}
				<button
					type="button"
					class="h-control-sm w-full cursor-pointer rounded-md border border-dashed border-line text-caption font-medium text-accent hover:bg-accent-muted"
					onclick={(e) => {
						e.stopPropagation()
						newColType = defaultColumnType(family)
						showAddCol = true
					}}
				>
					+ Add column
				</button>
			{:else}
				<div class="flex flex-col gap-2">
					<Input
						bind:value={newColName}
						placeholder="column_name"
						inputSize="md"
						class="cursor-text font-mono text-body"
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
							inputSize="md"
							placeholder="Column type"
						/>
					</div>
					<label
						class="flex cursor-pointer items-center gap-2 text-body text-fg-secondary"
					>
						<input
							type="checkbox"
							class="size-3.5 shrink-0 cursor-pointer accent-accent"
							bind:checked={newColPk}
						/>
						Primary key
					</label>
					<div class="flex gap-2 pt-0.5">
						<Button
							size="md"
							variant="primary"
							onclick={(e: MouseEvent) => {
								e.stopPropagation()
								onaddcolumn()
							}}
						>
							Add
						</Button>
						<Button
							size="md"
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
