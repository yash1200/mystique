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
	import { numericColumn } from '../cell-value'
	import { isLargeCell, largeCellHint } from '../large-cell'
	import { layoutColumns, type GridColumnLayout } from '../column-layout'
	import {
		autoColumnWidths,
		COL_W_MAX,
		COL_W_MIN,
		columnOffsets,
		columnWidth,
		dragWidth,
		fitColumnWidth,
	} from '../column-widths'
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
		columnLayout?: GridColumnLayout | null
		onColumnMenu?: (column: string, event: MouseEvent) => void
		/** User resized (or auto-fit) a column. Width is unscaled px. */
		onColumnWidth?: (column: string, width: number) => void
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
		columnLayout = null,
		onColumnMenu,
		onColumnWidth,
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
	/** Nominal column width; the scroll-direction unit only. */
	let colW = $state(128)
	/** UI scale: root font size / 16. Stored widths are unscaled. */
	let scale = $state(1)
	let headerH = $state(36)
	/** Live width while a header edge is dragged. */
	let resizing = $state<{ name: string; width: number } | null>(null)

	const rowH = $derived(appPrefs.gridRowHeightPx)
	const placedColumns = $derived(layoutColumns(table.columns, columnLayout))
	const frozenColumns = $derived(placedColumns.frozen)
	const scrollColumns = $derived(placedColumns.scroll)
	const totalCols = $derived(scrollColumns.length)
	const totalRows = $derived(pageEntries.length)

	// Automatic widths are measured once per table and column set, and again
	// when the first rows arrive. Paging and edits do not move the columns.
	let autoCache = { key: '', widths: {} as Record<string, number> }
	const autoWidths = $derived.by(() => {
		const key = [
			table.id,
			table.columns.map((column) => `${column.name}:${column.type}`).join('\u0001'),
			totalRows > 0,
		].join('\u0000')
		if (autoCache.key === key) return autoCache.widths
		const widths = untrack(() =>
			autoColumnWidths(
				table.columns,
				pageEntries.map((entry) => entry.row),
			),
		)
		autoCache = { key, widths }
		return widths
	})

	function baseWidth(name: string): number {
		if (resizing?.name === name) return resizing.width
		return columnWidth(name, columnLayout?.widths, autoWidths)
	}

	const frozenWidths = $derived(frozenColumns.map((c) => baseWidth(c.name) * scale))
	const scrollWidths = $derived(scrollColumns.map((c) => baseWidth(c.name) * scale))
	const frozenOffsets = $derived(columnOffsets(frozenWidths))
	const scrollOffsets = $derived(columnOffsets(scrollWidths))
	const frozenW = $derived(frozenOffsets[frozenOffsets.length - 1] ?? 0)
	const scrollW = $derived(scrollOffsets[scrollOffsets.length - 1] ?? 0)
	const totalW = $derived(indexW + frozenW + scrollW)
	const canvasW = $derived(Math.max(totalW, viewW))
	const bodyH = $derived(totalRows * rowH)
	const rowY = $derived(winStart * rowH)
	const leadW = $derived(scrollOffsets[Math.min(colStart, totalCols)] ?? 0)
	const tailW = $derived(
		Math.max(0, scrollW - (scrollOffsets[Math.min(colEnd, totalCols)] ?? scrollW)),
	)
	const rowsShown = $derived(
		pageEntries.slice(winStart, Math.min(winEnd, totalRows)),
	)
	const columnParts = $derived.by(() => {
		const parts: Array<
			| { key: string; spacer: number }
			| {
					key: string
					column: (typeof table.columns)[number]
					width: number
					left: number | null
			  }
		> = []
		frozenColumns.forEach((column, index) => {
			parts.push({
				key: `frozen:${column.name}`,
				column,
				width: frozenWidths[index] ?? colW,
				left: indexW + (frozenOffsets[index] ?? 0),
			})
		})
		if (leadW > 0) parts.push({ key: 'lead', spacer: leadW })
		const end = Math.min(colEnd, totalCols)
		for (let index = colStart; index < end; index++) {
			const column = scrollColumns[index]
			if (!column) continue
			parts.push({
				key: column.name,
				column,
				width: scrollWidths[index] ?? colW,
				left: null,
			})
		}
		if (tailW > 0) parts.push({ key: 'tail', spacer: tailW })
		return parts
	})

	/** Where a column sits in the canvas, for scroll-into-view. Screen px. */
	export function columnBox(
		name: string,
	): { left: number; width: number; pinned: number; frozen: boolean } | null {
		const pinned = indexW + frozenW
		const f = frozenColumns.findIndex((column) => column.name === name)
		if (f >= 0) {
			return {
				left: indexW + (frozenOffsets[f] ?? 0),
				width: frozenWidths[f] ?? colW,
				pinned,
				frozen: true,
			}
		}
		const s = scrollColumns.findIndex((column) => column.name === name)
		if (s < 0) return null
		return {
			left: pinned + (scrollOffsets[s] ?? 0),
			width: scrollWidths[s] ?? colW,
			pinned,
			frozen: false,
		}
	}

	function startResize(event: PointerEvent, name: string) {
		if (event.button !== 0 || !onColumnWidth) return
		event.preventDefault()
		event.stopPropagation()
		const handle = event.currentTarget as HTMLElement
		const start = baseWidth(name)
		const x0 = event.clientX
		let moved = false
		resizing = { name, width: start }
		handle.setPointerCapture?.(event.pointerId)
		const move = (e: PointerEvent) => {
			if (Math.abs(e.clientX - x0) > 1) moved = true
			resizing = { name, width: dragWidth(start, e.clientX - x0, scale) }
		}
		const end = () => {
			handle.removeEventListener('pointermove', move)
			handle.removeEventListener('pointerup', end)
			handle.removeEventListener('pointercancel', end)
			const done = resizing
			if (moved && done) onColumnWidth?.(name, done.width)
			resizing = null
		}
		handle.addEventListener('pointermove', move)
		handle.addEventListener('pointerup', end)
		handle.addEventListener('pointercancel', end)
	}

	function fitColumn(event: MouseEvent, column: (typeof table.columns)[number]) {
		event.preventDefault()
		event.stopPropagation()
		onColumnWidth?.(
			column.name,
			fitColumnWidth(
				column,
				pageEntries.map((entry) => entry.row),
			),
		)
	}

	function resizeByKey(event: KeyboardEvent, name: string) {
		const step = event.key === 'ArrowLeft' ? -16 : event.key === 'ArrowRight' ? 16 : 0
		if (!step || !onColumnWidth) return
		event.preventDefault()
		event.stopPropagation()
		onColumnWidth(name, dragWidth(baseWidth(name), step))
	}

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
			widths: scrollWidths,
			frozenW: indexW + frozenW,
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
		scale = root / 16
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
		void scrollWidths
		void frozenW
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
					{#each columnParts as part (part.key)}
						{#if 'spacer' in part}
							<th style="width: {part.spacer}px" aria-hidden="true"></th>
						{:else}
						{@const col = part.column}
						{@const active = sortColumn === col.name}
						<th
							class={cn(
								'border-b border-line px-2.5 py-1.5 text-caption font-semibold whitespace-nowrap text-fg-secondary',
								part.left != null ? 'sticky z-20 bg-elevated' : 'relative',
							)}
							style="width: {part.width}px;{part.left != null ? ` left: ${part.left}px;` : ''}"
							title={col.type}
							aria-sort={ariaSort(col.name)}
							oncontextmenu={(e) => {
								e.preventDefault()
								onColumnMenu?.(col.name, e)
							}}
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
							{#if onColumnWidth}
								<div
									class={cn(
										'gt-resize',
										resizing?.name === col.name && 'gt-resize-active',
									)}
									role="slider"
									tabindex="-1"
									aria-label="Width of {col.name}"
									aria-valuemin={COL_W_MIN}
									aria-valuemax={COL_W_MAX}
									aria-valuenow={Math.round(part.width / scale)}
									title="Drag to resize. Double-click to fit."
									onpointerdown={(e) => startResize(e, col.name)}
									ondblclick={(e) => fitColumn(e, col)}
									onkeydown={(e) => resizeByKey(e, col.name)}
								></div>
							{/if}
						</th>
						{/if}
					{/each}
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
							{#each columnParts as part (part.key)}
								{#if 'spacer' in part}
									<td style="width: {part.spacer}px" aria-hidden="true"></td>
								{:else}
								{@const col = part.column}
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
										part.left != null && 'sticky z-10',
										part.left != null &&
											(isDeleted
												? 'bg-danger/15'
												: isNew
													? 'bg-info/12'
													: rowSelected
														? 'bg-selected'
														: stripe
															? 'bg-panel'
															: 'bg-app'),
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
									style="width: {part.width}px; max-width: {part.width}px;{part.left != null ? ` left: ${part.left}px;` : ''}"
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
								{/if}
							{/each}
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

	/* Header edge handle: wider than it looks, drawn as a 2px line on hover. */
	.gt-resize {
		position: absolute;
		top: 0;
		right: -4px;
		bottom: 0;
		z-index: 1;
		width: 8px;
		cursor: col-resize;
		touch-action: none;
	}

	.gt-resize::after {
		content: '';
		position: absolute;
		top: 20%;
		bottom: 20%;
		left: 3px;
		width: 2px;
		border-radius: 1px;
		background: transparent;
	}

	.gt-resize:hover::after,
	.gt-resize:focus-visible::after,
	.gt-resize-active::after {
		background: var(--color-accent);
	}

	.gt-resize:focus-visible {
		outline: none;
	}
</style>
