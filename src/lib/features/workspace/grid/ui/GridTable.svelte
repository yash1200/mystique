<script lang="ts">
	/**
	 * Windowed table grid. Rows use `computeResultWindow`. Columns use
	 * `computeColumnWindow`. Cell text uses the 512-character display clip.
	 * The row object still holds the full value for the editor and the modal.
	 *
	 * #134 Chrome Beta headless, 1280×800, 80×100 with 12 KB JSON cells.
	 * Before, 8000 cells: mount 240ms, long tasks 137ms and 101ms (FunctionCall
	 * 138ms, Layout 52ms, Paint 17ms). Fling paint 163ms, layout 41ms.
	 * After, 1566 cells: mount 67ms, long tasks 54–70ms. Fling paint 32ms,
	 * layout 50ms, no long task over 100ms. The mount task was DOM, not a
	 * proxy scan, so row objects stay on the parent `$state`. A dirty value
	 * is read back from that row after its cell unmounts. The clip is display
	 * only; edit and the cell modal still see the full string.
	 */
	import type {
		ColumnReference,
		TableMeta,
		TablePreviewSortDir,
	} from '#core/contracts'
	import { Icon } from '#core/ui'
	import { cn } from '#core/util'
	import { appPrefs } from '#lib/app'
	import { untrack } from 'svelte'
	import { selectOptionsForColumn, usesSelectEditor } from '../column-editor'
	import { numericColumn } from '../filters'
	import { isLargeCell, largeCellHint } from '../large-cell'
	import {
		cellDisplay,
		computeColumnWindow,
		computeResultWindow,
		gridTrackPx,
	} from '../virtual'

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

	let scrollEl = $state<HTMLDivElement | null>(null)
	let winStart = $state(0)
	let winEnd = $state(48)
	let colStart = $state(0)
	let colEnd = $state(16)
	/** Not $state — a scroll-direction flip must not retrigger effects. */
	let scrollDirY: 1 | -1 = 1
	let scrollDirX: 1 | -1 = 1
	let lastTop = 0
	let lastLeft = 0
	let dirAccumY = 0
	let dirAccumX = 0
	let viewH = $state(640)
	let viewW = $state(1100)
	let indexW = $state(40)
	let colW = $state(128)
	let headerH = $state(36)

	const rowH = $derived(appPrefs.gridRowHeightPx)
	const totalCols = $derived(table.columns.length)
	const totalRows = $derived(pageEntries.length)
	const totalW = $derived(indexW + totalCols * colW)
	const canvasW = $derived(Math.max(totalW, viewW))
	const bodyH = $derived(totalRows * rowH)
	const rowY = $derived(winStart * rowH)
	const leadW = $derived(colStart * colW)
	const tailW = $derived(Math.max(0, (totalCols - colEnd) * colW))
	const rowsShown = $derived(
		pageEntries.slice(winStart, Math.min(winEnd, totalRows)),
	)
	const colsShown = $derived(
		table.columns.slice(colStart, Math.min(colEnd, totalCols)),
	)

	function sortLabel(name: string): string {
		if (sortColumn === name && sortDir === 'asc') return `Sort ${name} descending`
		if (sortColumn === name && sortDir === 'desc') return `Clear sort on ${name}`
		return `Sort ${name} ascending`
	}

	function ariaSort(name: string): 'none' | 'ascending' | 'descending' {
		if (sortColumn !== name || !sortDir) return 'none'
		return sortDir === 'desc' ? 'descending' : 'ascending'
	}

	function applyWindows(scrollTop: number, scrollLeft: number) {
		const rowNext = computeResultWindow({
			totalRows,
			scrollTop,
			viewportH: viewH,
			rowH,
			headerH,
			scrollDir: scrollDirY,
			current: { start: winStart, end: winEnd },
		})
		const colNext = computeColumnWindow({
			totalCols,
			scrollLeft,
			viewportW: viewW,
			colW,
			frozenW: indexW,
			scrollDir: scrollDirX,
			current: { start: colStart, end: colEnd },
		})
		if (rowNext.start !== winStart) winStart = rowNext.start
		if (rowNext.end !== winEnd) winEnd = rowNext.end
		if (colNext.start !== colStart) colStart = colNext.start
		if (colNext.end !== colEnd) colEnd = colNext.end
	}

	function bumpDir(
		delta: number,
		dir: 1 | -1,
		accum: number,
		unit: number,
	): { dir: 1 | -1; accum: number } {
		if (delta === 0) return { dir, accum }
		if (delta > 0 === dir > 0) return { dir, accum: 0 }
		const next = accum + Math.abs(delta)
		if (next > unit * 3) return { dir: delta > 0 ? 1 : -1, accum: 0 }
		return { dir, accum: next }
	}

	function onScroll() {
		const el = scrollEl
		if (!el) return
		const top = el.scrollTop
		const left = el.scrollLeft
		const y = bumpDir(top - lastTop, scrollDirY, dirAccumY, rowH)
		const x = bumpDir(left - lastLeft, scrollDirX, dirAccumX, colW)
		scrollDirY = y.dir
		scrollDirX = x.dir
		dirAccumY = y.accum
		dirAccumX = x.accum
		lastTop = top
		lastLeft = left
		applyWindows(top, left)
	}

	function measure() {
		const el = scrollEl
		if (!el) return
		viewH = el.clientHeight
		viewW = el.clientWidth
		const root =
			parseFloat(getComputedStyle(document.documentElement).fontSize) || 16
		const tracks = gridTrackPx(root)
		indexW = tracks.indexW
		colW = tracks.colW
		const head = el.querySelector('[data-grid-head]')
		if (head instanceof HTMLElement && head.offsetHeight > 0) {
			headerH = head.offsetHeight
		}
		untrack(() => applyWindows(el.scrollTop, el.scrollLeft))
	}

	$effect(() => {
		const el = scrollEl
		if (!el) return
		el.addEventListener('scroll', onScroll, { passive: true })
		untrack(measure)
		const ro =
			typeof ResizeObserver === 'undefined'
				? null
				: new ResizeObserver(() => measure())
		ro?.observe(el)
		return () => {
			el.removeEventListener('scroll', onScroll)
			ro?.disconnect()
		}
	})

	$effect(() => {
		void totalRows
		void totalCols
		void rowH
		untrack(() => {
			const el = scrollEl
			applyWindows(el?.scrollTop ?? 0, el?.scrollLeft ?? 0)
		})
	})
</script>

<div class="gt-scroll min-h-0 flex-1" data-grid-scroll bind:this={scrollEl}>
	<div class="gt-canvas" style="width: {canvasW}px">
		<table
			class="gt-table border-collapse bg-elevated text-left"
			data-grid-head
			style="width: {canvasW}px"
		>
			<thead class="sticky top-0 z-10">
				<tr>
					<th
						class="gt-idx sticky left-0 z-20 border-b border-r border-line bg-elevated px-2 py-1.5 text-caption font-semibold text-fg-muted"
						style="width: {indexW}px"
					>
						#
					</th>
					{#if leadW > 0}
						<th style="width: {leadW}px" aria-hidden="true"></th>
					{/if}
					{#each colsShown as col (col.name)}
						{@const active = sortColumn === col.name}
						<th
							class="border-b border-line px-2.5 py-1.5 text-caption font-semibold whitespace-nowrap text-fg-secondary"
							style="width: {colW}px"
							title={col.type}
							aria-sort={ariaSort(col.name)}
						>
							<div class="flex items-center gap-1.5">
								<span class="min-w-0 flex-1 truncate">
									<span class="text-fg">{col.name}</span>
									{#if col.pk}
										<span class="ml-1 font-semibold text-accent" title="Primary key">PK</span>
									{/if}
									<span class="ml-1.5 font-mono font-normal text-fg-muted">{col.type}</span>
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
					{#if tailW > 0}
						<th style="width: {tailW}px" aria-hidden="true"></th>
					{/if}
					<th class="w-full border-b border-line p-0" aria-hidden="true"></th>
				</tr>
			</thead>
		</table>

		<div class="gt-body" style="height: {bodyH}px">
			<table
				class="gt-table border-collapse text-left"
				style="width: {canvasW}px; transform: translate3d(0, {rowY}px, 0)"
			>
				<tbody>
					{#each rowsShown as entry (entry.index)}
						{@const i = entry.index}
						{@const row = entry.row}
						{@const rowSelected = selectedRowIndex === i || isRowSelected(i)}
						{@const stripe = i % 2 === 1}
						{@const isNew = isNewRow(i)}
						{@const isDeleted = isDeletedRow(i)}
						<tr
							class={cn(
								'border-b border-line/70',
								isDeleted && 'bg-danger/15',
								isDeleted && rowSelected && 'bg-danger/25',
								!isDeleted && isNew && 'bg-info/12',
								!isDeleted && isNew && rowSelected && 'bg-info/22',
								!isDeleted && !isNew && rowSelected && 'bg-selected',
								!isDeleted && !isNew && !rowSelected && stripe && 'bg-panel',
							)}
							style="height: {rowH}px"
							aria-selected={rowSelected}
						>
							<td
								class={cn(
									'gt-idx sticky left-0 z-10 cursor-pointer border-r border-line px-2 font-mono text-caption tabular-nums',
									isDeleted
										? 'bg-danger/15 font-medium text-danger'
										: isNew
											? 'bg-info/12 font-medium text-info'
											: rowSelected
												? 'bg-selected text-fg-muted'
												: stripe
													? 'bg-panel text-fg-muted'
													: 'bg-app text-fg-muted',
								)}
								style="width: {indexW}px"
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
							{#if leadW > 0}
								<td style="width: {leadW}px" aria-hidden="true"></td>
							{/if}
							{#each colsShown as col (col.name)}
								{@const raw = row[col.name]}
								{@const isNull = raw === null || raw === undefined}
								{@const shown = cellDisplay(raw)}
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
										'relative p-0 font-mono text-label',
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
									style="width: {colW}px; max-width: {colW}px"
									title={isEditing
										? undefined
										: dirty
											? `Edited · original: ${cellDisplay(originals[i]?.[col.name])}`
											: isLargeCell(raw)
												? largeCellHint(raw)
												: shown}
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
											'flex items-center gap-1 px-2.5',
											isEditing && 'invisible',
											isDeleted && 'line-through opacity-80',
											!isEditing && isNull && 'italic text-fg-muted',
											!isEditing && !isNull && !isDeleted && 'text-fg',
										)}
										style="height: {rowH}px"
										aria-hidden={isEditing}
									>
										<span
											class={cn(
												'min-w-0 flex-1 truncate',
												numericColumn(col.type) && 'text-right',
											)}
										>{shown}</span>
										{#if isLargeCell(raw)}
											<span
												class="shrink-0 font-sans text-caption tracking-normal text-fg-muted"
											>large</span>
										{/if}
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
												title={`Open ${ref.schema ? `${ref.schema}.` : ''}${ref.table}.${ref.column}`}
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
												<Icon name="external" class="pointer-events-none size-3.5" />
											</button>
										{/if}
									</div>
									{#if isEditing}
										{@const selectMode = usesSelectEditor(col, raw)}
										{#if selectMode}
											{@const options = selectOptionsForColumn(col, raw)}
											<select
												bind:this={inputEl}
												class="absolute inset-0 box-border h-full w-full border-none bg-input px-2 py-0.5 font-mono text-label text-fg focus:outline-none"
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
												class="absolute inset-0 box-border h-full w-full border-none bg-transparent px-2.5 font-mono text-label text-fg focus:outline-none"
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
							{#if tailW > 0}
								<td style="width: {tailW}px" aria-hidden="true"></td>
							{/if}
							<td class="w-full p-0" aria-hidden="true"></td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	</div>

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

<style>
	.gt-scroll {
		overflow: auto;
	}

	.gt-canvas {
		min-width: 100%;
	}

	.gt-table {
		table-layout: fixed;
	}

	.gt-canvas > .gt-table {
		position: sticky;
		top: 0;
		z-index: 10;
	}

	.gt-body {
		position: relative;
	}

	.gt-body .gt-table {
		position: absolute;
		top: 0;
		left: 0;
	}

	.gt-idx {
		box-sizing: border-box;
	}
</style>
