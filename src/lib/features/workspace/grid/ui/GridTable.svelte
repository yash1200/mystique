<script lang="ts">
	import type {
		ColumnReference,
		TableMeta,
		TablePreviewSortDir,
	} from '#core/contracts'
	import { Icon } from '#core/ui'
	import { cn } from '#core/util'
	import { selectOptionsForColumn, usesSelectEditor } from '../column-editor'
	import { cellText, numericColumn } from '../filters'
	export type CellRef = { row: number; col: string }

	interface Props {
		table: TableMeta
		pageEntries: { row: Record<string, unknown>; index: number }[]
		selectedRowIndex: number | null
		/** Extra rows in a Shift/⌘ full-row multi-selection. */
		isRowSelected?: (row: number) => boolean
		/** Cells in a Shift/⌘ cell multi-selection. */
		isCellSelected?: (row: number, col: string) => boolean
		/** Spreadsheet-style focused cell (absolute row index). */
		selectedCell: CellRef | null
		originals: Record<string, unknown>[]
		editing: CellRef | null
		draft?: string
		inputEl?: HTMLInputElement | HTMLSelectElement | null
		filtersActive: boolean
		hasActiveFilters: boolean
		isDirty: (row: number, col: string) => boolean
		/** Unsaved inserted / cloned rows (not yet written). */
		isNewRow?: (row: number) => boolean
		/** Rows marked for delete — still visible until Save. */
		isDeletedRow?: (row: number) => boolean
		onSelectCell: (row: number, col: string, e?: MouseEvent) => void
		onSelectRow: (row: number, e?: MouseEvent) => void
		onStartEdit: (row: number, col: string, e?: MouseEvent) => void
		onInputKeydown: (e: KeyboardEvent) => void
		onCommitEdit: () => void
		onRowMenu?: (row: number, e: MouseEvent) => void
		/** Right-click on a data cell. */
		onCellMenu?: (row: number, col: string, e: MouseEvent) => void
		/** Plugin-owned FK jump: open referenced table filtered to this value. */
		onOpenReference?: (ref: ColumnReference, value: unknown) => void
		/** Single-column preview sort (plugin ORDER BY / Mongo sort). */
		sortColumn?: string | null
		sortDir?: TablePreviewSortDir | null
		onSort?: (column: string) => void
	}

	let {
		table,
		pageEntries,
		selectedRowIndex,
		isRowSelected = () => false,
		isCellSelected = () => false,
		selectedCell,
		originals,
		editing,
		draft = $bindable(''),
		inputEl = $bindable<HTMLInputElement | HTMLSelectElement | null>(null),
		filtersActive,
		hasActiveFilters,
		isDirty,
		isNewRow = () => false,
		isDeletedRow = () => false,
		onSelectCell,
		onSelectRow,
		onStartEdit,
		onInputKeydown,
		onCommitEdit,
		onRowMenu,
		onCellMenu,
		onOpenReference,
		sortColumn = null,
		sortDir = null,
		onSort,
	}: Props = $props()

	function sortLabel(name: string): string {
		if (sortColumn === name && sortDir === 'asc')
			return `Sort ${name} descending`
		if (sortColumn === name && sortDir === 'desc')
			return `Clear sort on ${name}`
		return `Sort ${name} ascending`
	}

	function ariaSort(name: string): 'none' | 'ascending' | 'descending' {
		if (sortColumn !== name || !sortDir) return 'none'
		return sortDir === 'desc' ? 'descending' : 'ascending'
	}
</script>

<div class="min-h-0 flex-1 overflow-auto" data-grid-scroll>
	<table class="w-full border-collapse text-left">
		<thead class="sticky top-0 z-10">
			<tr class="bg-elevated">
				<th
					class="w-10 min-w-10 select-none border-b border-r border-line px-2 py-1.5 text-caption font-semibold text-fg-muted"
				>
					#
				</th>
				{#each table.columns as col (col.name)}
					{@const active = sortColumn === col.name}
					<th
						class="max-w-64 min-w-32 border-b border-line px-2.5 py-1.5 text-caption font-semibold whitespace-nowrap text-fg-secondary"
						title={col.type}
						aria-sort={ariaSort(col.name)}
					>
						<div class="flex items-center gap-1.5">
							<span class="min-w-0 flex-1 truncate">
								<span class="text-fg">{col.name}</span>
								{#if col.pk}
									<span
										class="ml-1 text-caption font-semibold text-accent"
										title="Primary key">PK</span
									>
								{/if}
								<span class="ml-1.5 font-mono font-normal text-fg-muted"
									>{col.type}</span
								>
							</span>
							{#if onSort}
								<button
									type="button"
									class={cn(
										'inline-grid size-5 shrink-0 place-items-center rounded-sm border border-transparent p-0 leading-none transition-colors',
										active
											? 'text-accent hover:bg-accent-muted'
											: 'text-fg-muted/70 hover:border-line hover:bg-hover hover:text-fg-secondary',
									)}
									title={sortLabel(col.name)}
									aria-label={sortLabel(col.name)}
									onclick={(e) => {
										e.stopPropagation()
										onSort(col.name)
									}}
								>
									<Icon
										name={active && sortDir === 'asc'
											? 'sort-asc'
											: active && sortDir === 'desc'
												? 'sort-desc'
												: 'sort-none'}
									/>
								</button>
							{/if}
						</div>
					</th>
				{/each}
				<!-- Absorbs leftover width so data columns keep a stable size. -->
				<th class="w-full min-w-0 border-b border-line p-0" aria-hidden="true"
				></th>
			</tr>
		</thead>
		<tbody>
			{#each pageEntries as entry (entry.index)}
				{@const i = entry.index}
				{@const row = entry.row}
				{@const rowSelected = selectedRowIndex === i || isRowSelected(i)}
				{@const stripe = i % 2 === 1}
				{@const isNew = isNewRow(i)}
				{@const isDeleted = isDeletedRow(i)}
				<tr
					class={cn(
						'border-b border-line/70 transition-colors',
						isDeleted && 'bg-danger/15',
						isDeleted && rowSelected && 'bg-danger/25',
						!isDeleted && isNew && 'bg-info/12',
						!isDeleted && isNew && rowSelected && 'bg-info/22',
						!isDeleted && !isNew && rowSelected && 'bg-selected',
						!isDeleted && !isNew && !rowSelected && stripe && 'bg-panel',
						!isDeleted && !isNew && !rowSelected && !stripe && 'bg-transparent',
					)}
					aria-selected={rowSelected}
				>
					<td
						class={cn(
							'cursor-pointer select-none border-r border-line px-2 py-1 font-mono text-caption tabular-nums hover:bg-hover',
							isDeleted
								? 'font-medium text-danger'
								: isNew
									? 'font-medium text-info'
									: 'text-fg-muted',
						)}
						title={isDeleted
							? 'Marked for delete — save to apply'
							: isNew
								? 'Unsaved new row'
								: undefined}
						onclick={(e) => {
							e.stopPropagation()
							onSelectRow(i, e)
						}}
						oncontextmenu={(e) => {
							e.preventDefault()
							e.stopPropagation()
							onRowMenu?.(i, e)
						}}
					>
						{#if isDeleted}
							<span class="sr-only">Marked for delete </span>
						{:else if isNew}
							<span class="sr-only">Unsaved new row </span>
						{/if}
						{i + 1}
					</td>
					{#each table.columns as col (col.name)}
						{@const raw = row[col.name]}
						{@const isNull = raw === null || raw === undefined}
						{@const dirty = isDirty(i, col.name)}
						{@const isEditing = editing?.row === i && editing.col === col.name}
						{@const isFocused =
							selectedCell?.row === i && selectedCell.col === col.name}
						{@const cellSelected = isCellSelected(i, col.name)}
						<td
							data-grid-cell
							data-row={i}
							data-col={col.name}
							class={cn(
								'relative max-w-64 min-w-32 border-l border-transparent p-0 font-mono text-label',
								isDeleted && 'text-danger/80',
								dirty &&
									!isDeleted &&
									!isEditing &&
									!cellSelected &&
									!isFocused &&
									'bg-warning/15 ring-1 ring-inset ring-warning/40',
								isEditing && 'bg-input ring-2 ring-inset ring-accent',
								isFocused &&
									!isEditing &&
									!isDeleted &&
									'bg-selected ring-2 ring-inset ring-accent',
								cellSelected &&
									!isFocused &&
									!isEditing &&
									!isDeleted &&
									'bg-selected',
								!isFocused &&
									!isEditing &&
									!dirty &&
									!isDeleted &&
									!cellSelected &&
									'hover:bg-hover',
							)}
							title={isEditing
								? undefined
								: dirty
									? `Edited · original: ${cellText(originals[i]?.[col.name])}`
									: cellText(raw)}
							onclick={(e) => {
								e.stopPropagation()
								onSelectCell(i, col.name, e)
							}}
							ondblclick={(e) => onStartEdit(i, col.name, e)}
							oncontextmenu={(e) => {
								e.preventDefault()
								e.stopPropagation()
								onCellMenu?.(i, col.name, e)
							}}
						>
							<div
								class={cn(
									'flex min-h-grid-row items-center gap-1 px-2.5 py-1',
									isEditing && 'invisible',
									isDeleted && 'line-through opacity-80',
									!isEditing && isNull && 'italic text-fg-muted',
									!isEditing && !isNull && !isDeleted && 'text-fg',
								)}
								aria-hidden={isEditing}
							>
								<span
									class={cn(
										'min-w-0 flex-1 truncate',
										numericColumn(col.type) && 'text-right',
									)}
								>{cellText(raw)}</span>
								{#if dirty}
									<span
										class="size-1.5 shrink-0 rounded-full bg-warning"
										title="Edited"
										aria-label="Edited"
									></span>
								{/if}
								{#if !isNull && !isDeleted && col.ref && onOpenReference}
									{@const ref = col.ref}
									<button
										type="button"
										class="inline-grid size-5 shrink-0 place-items-center rounded-sm border border-transparent bg-transparent p-0 leading-none text-fg-muted transition-colors hover:border-accent/30 hover:bg-accent-muted hover:text-accent"
										title={`Open ${ref.schema ? `${ref.schema}.` : ''}${ref.table}.${ref.column} = ${cellText(raw)}`}
										aria-label={`Open referenced ${ref.table} row`}
										onclick={(e) => {
											e.stopPropagation()
											e.preventDefault()
											onOpenReference(ref, raw)
										}}
										ondblclick={(e) => {
											e.stopPropagation()
											e.preventDefault()
										}}
									>
										<Icon
											name="external"
											class="pointer-events-none size-3.5"
										/>
									</button>
								{/if}
							</div>
							{#if isEditing}
								{@const selectMode = usesSelectEditor(col, raw)}
								{#if selectMode}
									{@const options = selectOptionsForColumn(col, raw)}
									<select
										bind:this={inputEl}
										class={cn(
											'absolute inset-0 box-border h-full w-full border-none bg-input px-2 py-0.5',
											'font-mono text-label text-fg focus:outline-none',
										)}
										bind:value={draft}
										onkeydown={onInputKeydown}
										onchange={onCommitEdit}
										onblur={onCommitEdit}
										onclick={(e) => e.stopPropagation()}
										aria-label="Edit {col.name}"
									>
										{#each options as opt (opt.value === '' ? '__null__' : opt.value)}
											<option value={opt.value}>{opt.label}</option>
										{/each}
									</select>
								{:else}
									<input
										bind:this={inputEl}
										class={cn(
											'absolute inset-0 box-border h-full w-full border-none bg-transparent px-2.5 py-1',
											'font-mono text-label text-fg focus:outline-none',
										)}
										bind:value={draft}
										onkeydown={onInputKeydown}
										onblur={onCommitEdit}
										onclick={(e) => e.stopPropagation()}
										aria-label="Edit {col.name}"
									/>
								{/if}
							{/if}
						</td>
					{/each}
					<td class="min-w-0 w-full p-0" aria-hidden="true"></td>
				</tr>
			{/each}
		</tbody>
	</table>

	{#if pageEntries.length === 0}
		<div class="px-4 py-8 text-center text-caption text-fg-muted">
			{#if filtersActive && hasActiveFilters}
				No rows match the current filters.
			{:else}
				No rows on this page.
			{/if}
		</div>
	{/if}
</div>
