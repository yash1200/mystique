<script lang="ts">
	/**
	 * Windowed results grid (read-only). The row window lives in
	 * `ResultRowWindow`; selection, keys and copy come from the shared grid
	 * core, so they behave as in the table grid.
	 */
	import type { QueryResult } from '#core/contracts'
	import { resultColumnTitle, type ResultColumnSource } from '#engines'
	import { ContextMenu } from '#core/ui'
	import { isMacPlatform, labelForKey } from '#features/shortcuts'
	import {
		CellValueModal,
		formatSelectionStats,
		GridSelection,
		gridKeyIntent,
		isLargeCell,
		isTypingTarget,
		largeCellFetchable,
		numericColumn,
		QUERY_UNKEYED_LARGE_CELL,
		selectionStats,
		valueToEditorText,
		type IdentStyle,
		type LargeLoader,
	} from '#features/workspace/data-grid'
	import { appPrefs } from '#lib/app'
	import { onMount, untrack } from 'svelte'
	import {
		copyColumnName,
		copyResultCells,
		copyResultRows,
		copyResultSelection,
		type ResultSource,
	} from '../results-copy'
	import {
		resultCellMenu,
		resultCellPick,
		resultRowMenu,
		resultRowPick,
	} from '../results-menus'
	import {
		RESULT_HEADER_H,
		binaryPreview,
		cellDisplay,
		measureColLayout,
	} from '#features/workspace/data-grid'
	import { ResultRowWindow } from '../results-window.svelte'
	/** Virtualization row height — follows density pref / `--grid-row-height`. */
	const ROW_H = $derived(appPrefs.gridRowHeightPx)
	const HEADER_H = RESULT_HEADER_H

	interface Props {
		columns: QueryResult['columns']
		/** Source table column per result column, when known (header tooltip). */
		sources?: (ResultColumnSource | null)[]
		identStyle?: IdentStyle
		/** Rows for the current page (may be up to 50k). */
		rows: unknown[][]
		/** Absolute index of rows[0] in the full result set (for # column). */
		pageStart: number
		/** Loads a large value in full. Absent when no session is live. */
		loadLarge?: LargeLoader | null
	}

	let {
		columns,
		sources = [],
		rows,
		pageStart,
		identStyle = 'postgres',
		loadLarge = null,
	}: Props = $props()

	function largeHint(cell: { locator?: unknown }): string {
		return cell.locator
			? 'Large value. Open the cell to load the full text.'
			: QUERY_UNKEYED_LARGE_CELL
	}

	const copyShortcut = `${labelForKey('mod', isMacPlatform())}+C`

	let scrollEl = $state<HTMLDivElement | null>(null)
	const totalRows = $derived(rows.length)
	const win = new ResultRowWindow({
		total: () => totalRows,
		rowH: () => ROW_H,
		headerH: HEADER_H,
	})
	/** Result columns are never hidden or moved: visible order is index order. */
	const colOrder = $derived(columns.map((_, i) => i))
	const colLayout = $derived.by(() =>
		measureColLayout(columns, rows, pageStart),
	)
	const windowIndices = $derived.by(() => {
		const start = Math.min(win.start, totalRows)
		const end = Math.min(win.end, totalRows)
		const n = Math.max(0, end - start)
		const out = new Array<number>(n)
		for (let i = 0; i < n; i++) out[i] = start + i
		return out
	})
	const bodyH = $derived(totalRows * ROW_H)
	const innerH = $derived(HEADER_H + bodyH)
	const windowY = $derived(win.start * ROW_H)

	const sel = new GridSelection<number>(() => colOrder)
	const source: ResultSource = {
		get columns() {
			return columns
		},
		get identStyle() {
			return identStyle
		},
		get loadLarge() {
			return loadLarge
		},
		rowCells,
	}
	let cellMenu = $state<{
		row: number
		col: number
		x: number
		y: number
	} | null>(null)
	let rowMenu = $state<{ row: number; x: number; y: number } | null>(null)
	let cellModal = $state<{ row: number; col: number } | null>(null)

	$effect(() => {
		const el = scrollEl
		if (!el) return
		return win.attach(el)
	})

	// Reset window when page / dataset changes
	$effect(() => {
		void pageStart
		void totalRows
		void columns.length
		void rows
		untrack(() => {
			sel.clear()
			cellMenu = null
			rowMenu = null
			cellModal = null
			win.reset(scrollEl)
		})
	})

	function pointerTarget(
		e: Event,
	):
		| { kind: 'cell'; row: number; col: number }
		| { kind: 'index'; row: number }
		| null {
		const t = e.target
		if (!(t instanceof Element)) return null
		const cell = t.closest('[data-result-cell]')
		if (cell instanceof HTMLElement && scrollEl?.contains(cell)) {
			const row = Number(cell.dataset.resultRow)
			const col = Number(cell.dataset.resultCol)
			if (Number.isFinite(row) && Number.isFinite(col)) {
				return { kind: 'cell', row, col }
			}
		}
		const idx = t.closest('[data-result-index]')
		if (idx instanceof HTMLElement && scrollEl?.contains(idx)) {
			const row = Number(idx.dataset.resultRow)
			if (Number.isFinite(row)) return { kind: 'index', row }
		}
		return null
	}

	function onGridClick(e: MouseEvent) {
		const hit = pointerTarget(e)
		if (!hit) return
		e.stopPropagation()
		if (hit.kind === 'index') {
			sel.pointRow(hit.row, e)
			reveal(hit.row, 'index')
		} else {
			const col = clampCol(hit.col)
			sel.pointCell(hit.row, col, e)
			reveal(hit.row, col)
		}
	}

	function onGridDblClick(e: MouseEvent) {
		const hit = pointerTarget(e)
		if (hit?.kind !== 'cell') return
		e.stopPropagation()
		openCellModal(hit.row, hit.col)
	}

	function onGridContextMenu(e: MouseEvent) {
		const hit = pointerTarget(e)
		if (!hit) return
		if (hit.kind === 'index') openRowMenu(hit.row, e)
		else openCellMenu(hit.row, hit.col, e)
	}

	function rowCells(absRow: number): unknown[] | null {
		const i = absRow - pageStart
		if (i < 0 || i >= rows.length) return null
		return rows[i] ?? null
	}

	function cellValue(absRow: number, col: number): unknown {
		return rowCells(absRow)?.[col]
	}

	function ensureVisible(absRow: number, col: number | 'index') {
		const el = scrollEl
		if (!el) return
		const local = absRow - pageStart
		if (local < 0 || local >= totalRows) return
		const top = local * ROW_H
		const bottom = top + ROW_H
		const viewSpan = Math.max(0, el.clientHeight - HEADER_H)
		if (top < el.scrollTop) el.scrollTop = top
		else if (bottom > el.scrollTop + viewSpan) el.scrollTop = Math.max(0, bottom - viewSpan)
		win.scrolled(el)

		// First data column (and row-index selection): pin to the left so
		// the # column is not clipped after scrolling in from the right.
		if (col === 0 || col === 'index') {
			el.scrollLeft = 0
			return
		}
		const target = el.querySelector(`[data-result-col="${col}"]`)
		if (!(target instanceof HTMLElement)) return
		const parent = el.getBoundingClientRect()
		const box = target.getBoundingClientRect()
		const pad = 8
		if (box.left < parent.left + pad) {
			el.scrollLeft += box.left - parent.left - pad
		} else if (box.right > parent.right - pad) {
			el.scrollLeft += box.right - parent.right + pad
		}
	}

	function clampCol(col: number): number {
		return Math.min(Math.max(0, col), Math.max(0, columns.length - 1))
	}

	/** Focus the grid and scroll a row (or one of its cells) into view. */
	function reveal(row: number, col: number | 'index') {
		scrollEl?.focus({ preventScroll: true })
		ensureVisible(row, col)
	}

	/** Reveal what a key left focused: the cell, or the row in row mode. */
	function revealSelection() {
		const cell = sel.cell
		if (sel.kind === 'cells' && cell) reveal(cell.row, cell.col)
		else if (sel.row != null) reveal(sel.row, 'index')
	}

	function selectCell(absRow: number, col: number) {
		sel.selectCell(absRow, clampCol(col))
		reveal(absRow, clampCol(col))
	}

	function openCellModal(absRow: number, col: number) {
		selectCell(absRow, col)
		cellMenu = null
		rowMenu = null
		cellModal = { row: absRow, col }
	}

	function openCellMenu(absRow: number, col: number, e: MouseEvent) {
		e.preventDefault()
		e.stopPropagation()
		rowMenu = null
		if (!sel.isCellSelected(absRow, col)) {
			selectCell(absRow, col)
		} else {
			sel.focus(absRow, clampCol(col))
			ensureVisible(absRow, clampCol(col))
		}
		cellMenu = { row: absRow, col, x: e.clientX, y: e.clientY }
	}

	function openRowMenu(absRow: number, e: MouseEvent) {
		e.preventDefault()
		e.stopPropagation()
		cellMenu = null
		if (!sel.isRowSelected(absRow)) {
			sel.selectRow(absRow)
			reveal(absRow, 'index')
		} else {
			sel.focusRow(absRow)
			ensureVisible(absRow, 'index')
		}
		rowMenu = { row: absRow, x: e.clientX, y: e.clientY }
	}

	const cellMenuItems = $derived(
		resultCellMenu(sel.kind === 'cells' && sel.cells.size > 1, copyShortcut),
	)
	const rowMenuItems = resultRowMenu(copyShortcut)

	function onCellMenuPick(id: string) {
		const ctx = cellMenu
		cellMenu = null
		const pick = resultCellPick(id)
		if (!ctx || !pick) return
		if (pick.kind === 'view') openCellModal(ctx.row, ctx.col)
		else if (pick.kind === 'cells') copyResultCells(source, sel, pick.format)
		else if (pick.kind === 'row') copyResultRows(source, [ctx.row], pick.format)
		else copyColumnName(source, ctx.col)
	}

	function onRowMenuPick(id: string) {
		const ctx = rowMenu
		rowMenu = null
		const format = resultRowPick(id)
		if (!ctx || !format) return
		// A row inside a multi-row selection copies the whole selection.
		const many = sel.isRowSelected(ctx.row) && sel.rows.size > 1
		copyResultRows(source, many ? sel.rowList() : [ctx.row], format)
	}

	function selectAllRows() {
		if (totalRows === 0) return
		sel.selectAll(Array.from({ length: totalRows }, (_, i) => pageStart + i), true)
		scrollEl?.focus({ preventScroll: true })
	}

	onMount(() => {
		const onKey = (e: KeyboardEvent) => {
			if (cellModal) return
			const t = e.target
			if (t instanceof HTMLElement) {
				if (t.closest('[role="dialog"]') || t.closest('[role="menu"]')) return
				if (isTypingTarget(t)) return
			}
			const inside = t instanceof Node && !!scrollEl?.contains(t)
			if (!inside && !sel.cell && sel.row == null && sel.rows.size === 0) return

			const intent = gridKeyIntent(e)
			switch (intent?.kind) {
				case 'select-all':
					e.preventDefault()
					selectAllRows()
					return
				case 'copy':
					e.preventDefault()
					copyResultSelection(source, sel)
					return
				case 'expand': {
					const cell = sel.cell
					if (!cell) return
					e.preventDefault()
					if (sel.kind === 'cells' && sel.cells.size > 1) return
					openCellModal(cell.row, cell.col)
					return
				}
				case 'escape':
					if (cellMenu || rowMenu) return
					e.preventDefault()
					if (sel.collapse()) revealSelection()
					else sel.clear()
					return
				case 'move':
					e.preventDefault()
					if (sel.move(intent, { first: pageStart, count: totalRows })) revealSelection()
					return
			}
		}
		window.addEventListener('keydown', onKey, true)
		return () => window.removeEventListener('keydown', onKey, true)
	})

	const modalValue = $derived.by(() => {
		if (!cellModal) return ''
		const v = cellValue(cellModal.row, cellModal.col)
		if (v === null || v === undefined) return 'NULL'
		return valueToEditorText(v)
	})
	const modalColumn = $derived(
		cellModal ? (columns[cellModal.col]?.name ?? '') : '',
	)
	const modalType = $derived(
		cellModal ? (columns[cellModal.col]?.dataType ?? '') : '',
	)
	const selectionLabel = $derived(
		formatSelectionStats(selectionStats(sel.values(cellValue, rowCells))),
	)
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
	class="qrg-scroll min-h-0 flex-1 outline-none"
	style="--qrg-row-h: {ROW_H}px; --qrg-cols: {colLayout.template}; --qrg-min-w: {colLayout.total}px"
	bind:this={scrollEl}
	tabindex="0"
	role="grid"
	aria-rowcount={totalRows}
	aria-colcount={columns.length + 1}
	aria-label="Query results"
	data-query-results-scroll
	onclick={onGridClick}
	ondblclick={onGridDblClick}
	oncontextmenu={onGridContextMenu}
>
	<div class="qrg-inner" style="height: {innerH}px">
		<div class="qrg-head bg-raised" role="row" style="height: {HEADER_H}px">
			<div
				data-result-index-col
				role="columnheader"
				class="qrg-cell qrg-idx border-b border-r border-line px-2 font-mono text-caption font-semibold text-fg-muted"
			>
				#
			</div>
			{#each columns as col, ci}
				<div
					data-result-col={ci}
					role="columnheader"
					class="qrg-cell border-b border-line px-2.5 font-mono text-caption font-semibold text-fg-secondary"
					title={resultColumnTitle(col.name, col.dataType, sources[ci])}
				>
					<span class="min-w-0 truncate">{col.name}</span>
					{#if col.dataType}
						<span class="ml-1 shrink-0 font-normal text-fg-muted"
							>{col.dataType}</span
						>
					{/if}
				</div>
			{/each}
		</div>

		<div
			class="qrg-body"
			style="height: {bodyH}px"
			aria-hidden={totalRows === 0}
		>
			<div class="qrg-window" style="transform: translate3d(0, {windowY}px, 0)">
				{#each windowIndices as local (pageStart + local)}
					{@const absolute = pageStart + local}
					{@const cells = rows[local]}
					<div
						class="qrg-row"
						style="height: {ROW_H}px"
						role="row"
						aria-rowindex={absolute + 1}
						aria-selected={sel.isRowSelected(absolute)}
					>
						<div
							data-result-index
							data-result-row={absolute}
							role="rowheader"
							class="qrg-cell qrg-idx border-b border-r border-line/60 border-r-line px-2 font-mono text-caption text-fg-muted tabular-nums"
						>
							{absolute + 1}
						</div>
						{#each columns as _col, ci}
							{@const cell = cells?.[ci]}
							{@const text = binaryPreview(cell, _col.dataType) ?? cellDisplay(cell)}
							{@const isFocused =
								sel.cell?.row === absolute && sel.cell.col === ci}
							<div
								data-result-cell
								data-result-col={ci}
								data-result-row={absolute}
								data-null={cell === null || cell === undefined ? '' : undefined}
								data-focus={isFocused ? '' : undefined}
								role="gridcell"
								aria-selected={isFocused || sel.isCellSelected(absolute, ci)}
								class="qrg-cell qrg-data border-b border-line/60 px-2.5 font-mono text-label {numericColumn(_col.dataType) ? 'text-right' : ''}"
								title={isLargeCell(cell) ? largeHint(cell) : text}
							>
								{text}
								{#if isLargeCell(cell)}
									<span class="ml-1 font-sans text-caption text-fg-muted">large</span>
								{/if}
							</div>
						{/each}
					</div>
				{/each}
			</div>
		</div>
	</div>
	{#if selectionLabel}
		<p class="m-0 shrink-0 border-t border-line px-2 py-1 text-caption text-fg-secondary tabular-nums">
			{selectionLabel}
		</p>
	{/if}
</div>

{#if cellModal}
	{@const raw = cellValue(cellModal.row, cellModal.col)}
	<CellValueModal
		column={modalColumn}
		columnType={modalType}
		rowIndex={cellModal.row}
		initialText={modalValue}
		readonly
		unfetchable={isLargeCell(raw) && (!largeCellFetchable(raw) || !loadLarge)}
		unfetchableText={QUERY_UNKEYED_LARGE_CELL}
		fetchChunk={isLargeCell(raw) && raw.locator && largeCellFetchable(raw) && loadLarge
			? (offset) => loadLarge(raw.locator!, offset)
			: undefined}
		onclose={() => (cellModal = null)}
	/>
{/if}

{#if rowMenu}
	<ContextMenu
		x={rowMenu.x}
		y={rowMenu.y}
		items={rowMenuItems}
		ariaLabel="Row actions"
		onpick={onRowMenuPick}
		onclose={() => (rowMenu = null)}
	/>
{/if}

{#if cellMenu}
	<ContextMenu
		x={cellMenu.x}
		y={cellMenu.y}
		items={cellMenuItems}
		ariaLabel="Cell actions"
		onpick={onCellMenuPick}
		onclose={() => (cellMenu = null)}
	/>
{/if}

<style>
	.qrg-scroll {
		overflow: auto;
	}

	.qrg-inner {
		position: relative;
		width: max(100%, var(--qrg-min-w));
		min-width: 100%;
	}

	.qrg-head,
	.qrg-row {
		display: grid;
		grid-template-columns: var(--qrg-cols);
		width: 100%;
	}

	.qrg-head {
		position: sticky;
		top: 0;
		z-index: 10;
	}

	/*
	 * No striping on the body box: it is `totalRows * rowH` tall (140k px on a
	 * full page) and a background there is the one paint cost in the grid that
	 * grows with the result size. Rows draw their own bottom border instead.
	 */
	.qrg-body {
		position: relative;
	}

	.qrg-window {
		position: absolute;
		top: 0;
		left: 0;
		right: 0;
	}

	.qrg-row {
		contain: layout style paint;
	}

	.qrg-row[aria-selected='true'] {
		background-color: var(--color-selected);
	}

	.qrg-row:not([aria-selected='true']):hover {
		background-color: var(--color-hover);
	}

	/*
	 * One node per cell: `line-height` centers the single text line, so the
	 * flex box and the inner truncating <span> every cell used to mount are
	 * both gone. Selection and NULL state are attributes styled below rather
	 * than a rebuilt class string, so a click patches one attribute per cell
	 * instead of a fresh class list.
	 */
	.qrg-cell {
		min-width: 0;
		height: 100%;
		box-sizing: border-box;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		line-height: calc(var(--qrg-row-h) - 1px);
		cursor: pointer;
		user-select: none;
	}

	.qrg-head .qrg-cell {
		display: flex;
		align-items: center;
		line-height: normal;
		cursor: default;
	}

	.qrg-data {
		color: var(--color-fg);
	}

	.qrg-data[data-null] {
		font-style: italic;
		color: var(--color-fg-muted);
	}

	.qrg-data[aria-selected='true'] {
		background-color: var(--color-selected);
	}

	.qrg-data[data-focus] {
		box-shadow: inset 0 0 0 2px var(--color-accent);
	}
</style>
