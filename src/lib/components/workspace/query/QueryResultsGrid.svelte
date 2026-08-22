<script lang="ts">
	/**
	 * Windowed (virtualized) results table.
	 * Large directional overscan keeps rows ahead of the scroll so spacers
	 * rarely flash empty while the user is moving through big result sets.
	 */
	import type { QueryResult } from '$lib/api/engines'
	import { ContextMenu, type ContextMenuItem } from '$lib/components/core'
	import { isMacPlatform, labelForKey } from '$lib/data/appShortcuts'
	import { appPrefs } from '$lib/stores/app-prefs.svelte'
	import { cn } from '$lib/utils/cn'
	import {
		buildJson,
		buildMarkdown,
		buildTsv,
		copyTextToClipboard,
	} from '$lib/utils/exportTable'
	import { onMount, untrack } from 'svelte'
	import CellValueModal from '../grid/CellValueModal.svelte'
	import { valueToEditorText } from '../grid/cellView'

	/** Virtualization row height — follows density pref / `--grid-row-height`. */
	const ROW_H = $derived(appPrefs.gridRowHeightPx)
	const HEADER_H = 32
	/** Minimum rows buffered on each side of the viewport. */
	const OVERSCAN_MIN = 200
	/**
	 * Extra buffer as multiples of the visible row count.
	 * ~2.5 screens ahead of motion absorbs fast wheel / trackpad flings
	 * without mounting tens of thousands of nodes.
	 */
	const OVERSCAN_VIEWPORTS = 2.5
	/** Extra multiplier applied in the direction the user is scrolling. */
	const DIRECTION_BIAS = 1.75

	interface Props {
		columns: QueryResult['columns']
		/** Rows for the current page (may be up to 50k). */
		rows: unknown[][]
		/** Absolute index of rows[0] in the full result set (for # column). */
		pageStart: number
		onstatus?: (message: string) => void
	}

	let { columns, rows, pageStart, onstatus }: Props = $props()

	const mac = isMacPlatform()
	const copyShortcut = `${labelForKey('mod', mac)}+C`

	let scrollEl = $state<HTMLDivElement | null>(null)
	let scrollTop = $state(0)
	let viewportH = $state(360)
	/** +1 scrolling down, -1 scrolling up. */
	let scrollDir = $state(1)

	const colCount = $derived(columns.length + 1)
	const totalRows = $derived(rows.length)
	const exportCols = $derived(columns.map((c) => ({ name: c.name })))

	type CellRef = { row: number; col: number }
	let selectedCell = $state<CellRef | null>(null)
	let selectedRow = $state<number | null>(null)
	let selectedRows = $state<Set<number>>(new Set())
	let selectionAnchor = $state<number | null>(null)
	let selectedCells = $state<Set<string>>(new Set())
	let cellAnchor = $state<CellRef | null>(null)
	let selKind = $state<'none' | 'rows' | 'cells'>('none')
	let cellMenu = $state<{
		row: number
		col: number
		x: number
		y: number
	} | null>(null)
	let rowMenu = $state<{ row: number; x: number; y: number } | null>(null)
	let cellModal = $state<CellRef | null>(null)

	const windowRange = $derived.by(() => {
		const n = totalRows
		if (n === 0) return { start: 0, end: 0, padTop: 0, padBottom: 0 }

		const usable = Math.max(
			viewportH > HEADER_H ? viewportH - HEADER_H : viewportH,
			ROW_H * 8,
		)
		const first = Math.max(0, Math.floor(scrollTop / ROW_H))
		const visibleCount = Math.ceil(usable / ROW_H) + 1

		const baseOverscan = Math.max(
			OVERSCAN_MIN,
			Math.ceil(visibleCount * OVERSCAN_VIEWPORTS),
		)
		// Prefer buffer ahead of the scroll so fling/trackpad lag stays covered.
		const overscanBefore =
			scrollDir < 0 ? Math.ceil(baseOverscan * DIRECTION_BIAS) : baseOverscan
		const overscanAfter =
			scrollDir > 0 ? Math.ceil(baseOverscan * DIRECTION_BIAS) : baseOverscan

		const start = Math.max(0, first - overscanBefore)
		const end = Math.min(n, first + visibleCount + overscanAfter)
		return {
			start,
			end,
			padTop: start * ROW_H,
			padBottom: Math.max(0, (n - end) * ROW_H),
		}
	})

	const windowRows = $derived.by(() => {
		const { start, end } = windowRange
		const out: { absolute: number; cells: unknown[] }[] = []
		for (let i = start; i < end; i++) {
			out.push({ absolute: pageStart + i, cells: rows[i]! })
		}
		return out
	})

	function onScroll() {
		const el = scrollEl
		if (!el) return
		const next = el.scrollTop
		// Ignore sub-pixel jitter; still update direction on meaningful moves.
		if (next > scrollTop + 1) scrollDir = 1
		else if (next < scrollTop - 1) scrollDir = -1
		// Apply immediately so the window moves with the thumb, not a frame later.
		scrollTop = next
	}

	$effect(() => {
		const el = scrollEl
		if (!el) return
		const measure = () => {
			viewportH = el.clientHeight
		}
		measure()
		if (typeof ResizeObserver === 'undefined') return
		const ro = new ResizeObserver(measure)
		ro.observe(el)
		return () => ro.disconnect()
	})

	// Reset window when page / dataset changes
	$effect(() => {
		void pageStart
		void totalRows
		void columns.length
		scrollTop = 0
		scrollDir = 1
		clearAllSelection()
		cellMenu = null
		rowMenu = null
		cellModal = null
		untrack(() => {
			if (scrollEl) scrollEl.scrollTop = 0
		})
	})

	function cellDisplay(v: unknown): string {
		if (v === null || v === undefined) return 'NULL'
		if (typeof v === 'object') {
			try {
				return JSON.stringify(v)
			} catch {
				return String(v)
			}
		}
		return String(v)
	}

	function localIndex(absRow: number): number {
		return absRow - pageStart
	}

	function rowCells(absRow: number): unknown[] | null {
		const i = localIndex(absRow)
		if (i < 0 || i >= rows.length) return null
		return rows[i] ?? null
	}

	function cellValue(absRow: number, col: number): unknown {
		return rowCells(absRow)?.[col]
	}

	function focusGrid() {
		scrollEl?.focus({ preventScroll: true })
	}

	function ensureVisible(absRow: number, col?: number | 'index') {
		const el = scrollEl
		if (!el) return
		const local = localIndex(absRow)
		if (local < 0 || local >= totalRows) return
		const top = local * ROW_H
		const bottom = top + ROW_H
		const viewTop = el.scrollTop
		const viewH = Math.max(0, el.clientHeight - HEADER_H)
		const viewBottom = viewTop + viewH
		if (top < viewTop) el.scrollTop = top
		else if (bottom > viewBottom) el.scrollTop = Math.max(0, bottom - viewH)

		if (col === undefined) return
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

	function selCellKey(row: number, col: number): string {
		return `${row}::${col}`
	}

	function clampCol(col: number): number {
		return Math.min(Math.max(0, col), Math.max(0, columns.length - 1))
	}

	function clampRow(absRow: number): number {
		const last = pageStart + Math.max(0, totalRows - 1)
		return Math.min(Math.max(pageStart, absRow), last)
	}

	function clearCellSelection() {
		selectedCells = new Set()
		cellAnchor = null
	}

	function clearRowSelection() {
		selectedRows = new Set()
		selectionAnchor = null
		selectedRow = null
	}

	function clearAllSelection() {
		selKind = 'none'
		selectedCell = null
		clearCellSelection()
		clearRowSelection()
	}

	function setExclusiveCell(absRow: number, col: number) {
		const nextCol = clampCol(col)
		selKind = 'cells'
		clearRowSelection()
		selectedCell = { row: absRow, col: nextCol }
		cellAnchor = { row: absRow, col: nextCol }
		selectedCells = new Set([selCellKey(absRow, nextCol)])
		focusGrid()
		ensureVisible(absRow, nextCol)
	}

	function toggleCell(absRow: number, col: number) {
		const nextCol = clampCol(col)
		selKind = 'cells'
		clearRowSelection()
		const key = selCellKey(absRow, nextCol)
		const next = new Set(selectedCells)
		if (next.has(key)) next.delete(key)
		else next.add(key)
		if (next.size === 0) next.add(key)
		selectedCells = next
		selectedCell = { row: absRow, col: nextCol }
		cellAnchor = { row: absRow, col: nextCol }
		focusGrid()
		ensureVisible(absRow, nextCol)
	}

	function selectCellRect(toRow: number, toCol: number) {
		selKind = 'cells'
		clearRowSelection()
		const from = cellAnchor ?? selectedCell ?? { row: toRow, col: toCol }
		const r0 = Math.min(from.row, toRow)
		const r1 = Math.max(from.row, toRow)
		const c0 = Math.min(from.col, toCol)
		const c1 = Math.max(from.col, toCol)
		const next = new Set<string>()
		for (let r = r0; r <= r1; r++) {
			for (let c = c0; c <= c1; c++) next.add(selCellKey(r, c))
		}
		selectedCells = next
		selectedCell = { row: toRow, col: toCol }
		focusGrid()
		ensureVisible(toRow, toCol)
	}

	function applyCellPointer(absRow: number, col: number, e?: MouseEvent) {
		const mod = !!(e && (e.metaKey || e.ctrlKey))
		const shift = !!(e && e.shiftKey)
		if (shift) selectCellRect(absRow, clampCol(col))
		else if (mod) toggleCell(absRow, col)
		else setExclusiveCell(absRow, col)
	}

	function setExclusiveRow(absRow: number) {
		selKind = 'rows'
		clearCellSelection()
		selectedCell = null
		selectedRows = new Set([absRow])
		selectionAnchor = absRow
		selectedRow = absRow
		focusGrid()
		ensureVisible(absRow, 'index')
	}

	function toggleRow(absRow: number) {
		selKind = 'rows'
		clearCellSelection()
		selectedCell = null
		const next = new Set(selectedRows)
		if (next.has(absRow)) next.delete(absRow)
		else next.add(absRow)
		if (next.size === 0) next.add(absRow)
		selectedRows = next
		selectionAnchor = absRow
		selectedRow = absRow
		focusGrid()
		ensureVisible(absRow, 'index')
	}

	function selectRowRange(to: number) {
		selKind = 'rows'
		clearCellSelection()
		selectedCell = null
		const from = selectionAnchor ?? selectedRow ?? to
		const lo = Math.min(from, to)
		const hi = Math.max(from, to)
		const next = new Set<number>()
		for (let i = lo; i <= hi; i++) next.add(i)
		selectedRows = next
		selectedRow = to
		focusGrid()
		ensureVisible(to, 'index')
	}

	function applyRowPointer(absRow: number, e?: MouseEvent) {
		const mod = !!(e && (e.metaKey || e.ctrlKey))
		const shift = !!(e && e.shiftKey)
		if (shift) selectRowRange(absRow)
		else if (mod) toggleRow(absRow)
		else setExclusiveRow(absRow)
	}

	function isRowSelected(absRow: number): boolean {
		return selKind === 'rows' && selectedRows.has(absRow)
	}

	function isCellSelected(absRow: number, col: number): boolean {
		return selKind === 'cells' && selectedCells.has(selCellKey(absRow, col))
	}

	function selectedRowList(): number[] {
		return [...selectedRows].sort((a, b) => a - b)
	}

	function openCellModal(absRow: number, col: number) {
		setExclusiveCell(absRow, col)
		cellMenu = null
		rowMenu = null
		cellModal = { row: absRow, col }
	}

	function openCellMenu(absRow: number, col: number, e: MouseEvent) {
		e.preventDefault()
		e.stopPropagation()
		rowMenu = null
		if (selKind !== 'cells' || !selectedCells.has(selCellKey(absRow, col))) {
			setExclusiveCell(absRow, col)
		} else {
			selectedCell = { row: absRow, col: clampCol(col) }
			ensureVisible(absRow, selectedCell.col)
		}
		cellMenu = { row: absRow, col, x: e.clientX, y: e.clientY }
	}

	function openRowMenu(absRow: number, e: MouseEvent) {
		e.preventDefault()
		e.stopPropagation()
		cellMenu = null
		if (selKind !== 'rows' || !selectedRows.has(absRow)) {
			setExclusiveRow(absRow)
		} else {
			selectedRow = absRow
			ensureVisible(absRow, 'index')
		}
		rowMenu = { row: absRow, x: e.clientX, y: e.clientY }
	}

	async function flashCopy(ok: boolean, okMsg: string) {
		onstatus?.(ok ? okMsg : 'Copy failed')
	}

	function cellClipboardText(v: unknown): string {
		if (v === null || v === undefined) return ''
		if (typeof v === 'object') {
			try {
				return JSON.stringify(v)
			} catch {
				return String(v)
			}
		}
		return String(v)
	}

	async function copyCellValue(absRow: number, col: number) {
		const res = await copyTextToClipboard(
			cellClipboardText(cellValue(absRow, col)),
		)
		await flashCopy(res.ok, 'Cell copied')
	}

	async function copyColumnName(col: number) {
		const name = columns[col]?.name ?? ''
		const res = await copyTextToClipboard(name)
		await flashCopy(res.ok, 'Column name copied')
	}

	async function copyCellJson(absRow: number, col: number) {
		const v = cellValue(absRow, col)
		let text: string
		try {
			text = `${JSON.stringify(v === undefined ? null : v, null, 2)}\n`
		} catch {
			text = String(v)
		}
		const res = await copyTextToClipboard(text)
		await flashCopy(res.ok, 'Copied as JSON')
	}

	async function copyCellTsv(absRow: number, col: number) {
		const colDef = exportCols[col] ?? { name: columns[col]?.name ?? 'value' }
		const text = buildTsv([colDef], [[cellValue(absRow, col)]])
		const res = await copyTextToClipboard(text)
		await flashCopy(res.ok, 'Copied as TSV')
	}

	function cellSelectionPayload(): {
		columns: { name: string }[]
		cells: unknown[][]
		count: number
	} | null {
		if (selKind !== 'cells' || selectedCells.size === 0) return null
		const parsed: { row: number; ci: number }[] = []
		for (const key of selectedCells) {
			const sep = key.lastIndexOf('::')
			if (sep < 0) continue
			const row = Number(key.slice(0, sep))
			const ci = Number(key.slice(sep + 2))
			if (!Number.isFinite(row) || !Number.isFinite(ci)) continue
			parsed.push({ row, ci })
		}
		if (parsed.length === 0) return null
		const rowIdx = [...new Set(parsed.map((p) => p.row))].sort((a, b) => a - b)
		const colIdx = [...new Set(parsed.map((p) => p.ci))].sort((a, b) => a - b)
		const cols = colIdx.map((i) => exportCols[i] ?? { name: `col_${i}` })
		const cells = rowIdx.map((r) =>
			colIdx.map((c) =>
				selectedCells.has(selCellKey(r, c)) ? (cellValue(r, c) ?? '') : '',
			),
		)
		return { columns: cols, cells, count: parsed.length }
	}

	async function copyCellSelection(kind: 'copy' | 'json' | 'tsv') {
		const payload = cellSelectionPayload()
		if (!payload) {
			if (selectedCell) {
				if (kind === 'json')
					return copyCellJson(selectedCell.row, selectedCell.col)
				if (kind === 'tsv')
					return copyCellTsv(selectedCell.row, selectedCell.col)
				return copyCellValue(selectedCell.row, selectedCell.col)
			}
			return
		}
		if (payload.count === 1 && selectedCell) {
			if (kind === 'json')
				return copyCellJson(selectedCell.row, selectedCell.col)
			if (kind === 'tsv') return copyCellTsv(selectedCell.row, selectedCell.col)
			return copyCellValue(selectedCell.row, selectedCell.col)
		}
		let text = ''
		let okMsg = `${payload.count} cells copied`
		if (kind === 'json') {
			text = buildJson(payload.columns, payload.cells, true)
			okMsg = `Copied ${payload.count} cells as JSON`
		} else {
			text = buildTsv(payload.columns, payload.cells)
			okMsg =
				kind === 'tsv'
					? `Copied ${payload.count} cells as TSV`
					: `${payload.count} cells copied`
		}
		const res = await copyTextToClipboard(text)
		await flashCopy(res.ok, okMsg)
	}

	async function copyRowAs(
		indices: number[],
		kind: 'copy' | 'json' | 'markdown',
	) {
		const rowsOut: unknown[][] = []
		for (const abs of indices) {
			const cells = rowCells(abs)
			if (cells) rowsOut.push(cells)
		}
		if (rowsOut.length === 0) return
		const n = rowsOut.length
		let text = ''
		let okMsg = n === 1 ? 'Row copied' : `${n} rows copied`
		if (kind === 'copy') {
			text = buildTsv(exportCols, rowsOut)
		} else if (kind === 'json') {
			text = buildJson(exportCols, rowsOut, true)
			okMsg = n === 1 ? 'Copied as JSON' : `Copied ${n} as JSON`
		} else {
			text = buildMarkdown(exportCols, rowsOut)
			okMsg = n === 1 ? 'Copied as Markdown' : `Copied ${n} as Markdown`
		}
		const res = await copyTextToClipboard(text)
		await flashCopy(res.ok, okMsg)
	}

	function targetRows(primary: number): number[] {
		if (
			selKind === 'rows' &&
			selectedRows.has(primary) &&
			selectedRows.size > 1
		) {
			return selectedRowList()
		}
		return [primary]
	}

	const cellMenuItems = $derived.by((): ContextMenuItem[] => {
		const n = selKind === 'cells' ? selectedCells.size : 1
		const many = n > 1
		return [
			{
				id: 'view',
				label: 'View in modal',
				shortcut: '⇧Enter',
				disabled: many,
			},
			{
				id: 'copy',
				label: many ? `Copy ${n} cells` : 'Copy',
				shortcut: copyShortcut,
				separatorBefore: true,
			},
			{ id: 'copy-column', label: 'Copy column name' },
			{
				id: 'copy-json',
				label: many ? `Copy ${n} as JSON` : 'Copy JSON',
			},
			{
				id: 'copy-tsv',
				label: many ? `Copy ${n} as TSV` : 'Copy TSV',
			},
		]
	})

	const rowMenuItems = $derived.by((): ContextMenuItem[] => {
		const n =
			selKind === 'rows' && selectedRows.size > 0 ? selectedRows.size : 1
		const many = n > 1
		return [
			{
				id: 'copy',
				label: many ? `Copy ${n} rows` : 'Copy',
				shortcut: copyShortcut,
			},
			{
				id: 'copy-json',
				label: many ? `Copy ${n} as JSON` : 'Copy as JSON',
			},
			{
				id: 'copy-markdown',
				label: many ? `Copy ${n} as Markdown` : 'Copy as Markdown',
			},
		]
	})

	function onCellMenuPick(id: string) {
		const ctx = cellMenu
		cellMenu = null
		if (!ctx) return
		if (id === 'view') {
			openCellModal(ctx.row, ctx.col)
			return
		}
		if (id === 'copy') void copyCellSelection('copy')
		if (id === 'copy-column') void copyColumnName(ctx.col)
		if (id === 'copy-json') void copyCellSelection('json')
		if (id === 'copy-tsv') void copyCellSelection('tsv')
	}

	function onRowMenuPick(id: string) {
		const ctx = rowMenu
		rowMenu = null
		if (!ctx) return
		const targets = targetRows(ctx.row)
		if (id === 'copy') void copyRowAs(targets, 'copy')
		if (id === 'copy-json') void copyRowAs(targets, 'json')
		if (id === 'copy-markdown') void copyRowAs(targets, 'markdown')
	}

	function moveSelection(dRow: number, dCol: number, extend = false) {
		if (totalRows === 0 || columns.length === 0) return
		const from = selectedCell ?? {
			row: selectedRow ?? pageStart,
			col: 0,
		}
		const nextRow = clampRow(from.row + dRow)
		const nextCol = clampCol(from.col + dCol)
		if (selKind === 'rows' || !selectedCell) {
			if (dCol !== 0 && !extend) setExclusiveCell(nextRow, nextCol)
			else if (!extend) setExclusiveRow(nextRow)
			else selectRowRange(nextRow)
			return
		}
		if (!extend) setExclusiveCell(nextRow, nextCol)
		else selectCellRect(nextRow, nextCol)
	}

	function selectAllRows() {
		if (totalRows === 0) return
		selKind = 'rows'
		clearCellSelection()
		selectedCell = null
		const next = new Set<number>()
		for (let i = 0; i < totalRows; i++) next.add(pageStart + i)
		selectedRows = next
		if (selectionAnchor == null) selectionAnchor = pageStart
		selectedRow = selectedRow ?? pageStart
		focusGrid()
		onstatus?.(
			next.size === 1 ? '1 row selected' : `${next.size} rows selected`,
		)
	}

	onMount(() => {
		const onKey = (e: KeyboardEvent) => {
			if (cellModal) return
			const t = e.target
			if (t instanceof HTMLElement) {
				if (t.closest('[role="dialog"]') || t.closest('[role="menu"]')) return
				if (
					t.tagName === 'INPUT' ||
					t.tagName === 'TEXTAREA' ||
					t.tagName === 'SELECT' ||
					t.isContentEditable
				) {
					return
				}
			}
			const inside = t instanceof Node && !!scrollEl?.contains(t)
			if (
				!inside &&
				!selectedCell &&
				selectedRow == null &&
				selectedRows.size === 0
			) {
				return
			}

			const mod = e.metaKey || e.ctrlKey
			if (mod && !e.altKey && !e.shiftKey && (e.key === 'a' || e.key === 'A')) {
				e.preventDefault()
				selectAllRows()
				return
			}
			if (mod && !e.altKey && !e.shiftKey && (e.key === 'c' || e.key === 'C')) {
				e.preventDefault()
				if (selKind === 'cells' && selectedCells.size > 1) {
					void copyCellSelection('copy')
				} else if (selKind === 'rows' && selectedRows.size > 0) {
					void copyRowAs(selectedRowList(), 'copy')
				} else if (selectedCell) {
					void copyCellValue(selectedCell.row, selectedCell.col)
				}
				return
			}
			if (e.key === 'Enter' && e.shiftKey && selectedCell) {
				e.preventDefault()
				if (selKind === 'cells' && selectedCells.size > 1) return
				openCellModal(selectedCell.row, selectedCell.col)
				return
			}
			if (e.key === 'Escape') {
				if (cellMenu || rowMenu) return
				e.preventDefault()
				if (selKind === 'cells' && selectedCells.size > 1 && selectedCell) {
					setExclusiveCell(selectedCell.row, selectedCell.col)
					return
				}
				if (
					selKind === 'rows' &&
					selectedRows.size > 1 &&
					selectedRow != null
				) {
					setExclusiveRow(selectedRow)
					return
				}
				clearAllSelection()
				return
			}
			if (e.key === 'ArrowUp') {
				e.preventDefault()
				moveSelection(-1, 0, e.shiftKey)
			} else if (e.key === 'ArrowDown') {
				e.preventDefault()
				moveSelection(1, 0, e.shiftKey)
			} else if (e.key === 'ArrowLeft') {
				e.preventDefault()
				moveSelection(0, -1, e.shiftKey)
			} else if (e.key === 'ArrowRight') {
				e.preventDefault()
				moveSelection(0, 1, e.shiftKey)
			} else if (e.key === 'Tab') {
				e.preventDefault()
				moveSelection(0, e.shiftKey ? -1 : 1)
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
</script>

<div
	class="min-h-0 flex-1 overflow-auto outline-none"
	bind:this={scrollEl}
	onscroll={onScroll}
	tabindex="0"
	role="grid"
	aria-label="Query results"
	data-query-results-scroll
>
	<table class="w-max min-w-full border-collapse text-left">
		<thead class="sticky top-0 z-10 bg-elevated">
			<tr style="height: {HEADER_H}px">
				<th
					data-result-index-col
					class="border-b border-r border-line px-2 py-1.5 font-mono text-caption font-semibold text-fg-muted"
					>#</th
				>
				{#each columns as col, ci (col.name + '\0' + ci)}
					<th
						data-result-col={ci}
						class="border-b border-line px-2.5 py-1.5 font-mono text-caption font-semibold whitespace-nowrap text-fg-secondary"
						title={col.dataType ?? undefined}
					>
						{col.name}
						{#if col.dataType}
							<span class="ml-1 font-normal text-fg-muted">{col.dataType}</span>
						{/if}
					</th>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#if windowRange.padTop > 0}
				<tr aria-hidden="true" class="pointer-events-none">
					<td
						colspan={colCount}
						class="border-0 p-0"
						style="height: {windowRange.padTop}px; line-height: 0; font-size: 0"
					>
						<div style="height: {windowRange.padTop}px"></div>
					</td>
				</tr>
			{/if}

			{#each windowRows as item (item.absolute)}
				{@const rowSelected = isRowSelected(item.absolute)}
				<tr
					class={cn(rowSelected ? 'bg-selected' : 'hover:bg-hover')}
					style="height: {ROW_H}px"
					aria-selected={rowSelected}
				>
					<td
						class="cursor-pointer select-none border-b border-line/60 border-r border-r-line px-2 align-middle font-mono text-caption text-fg-muted tabular-nums hover:bg-hover"
						style="height: {ROW_H}px; max-height: {ROW_H}px; padding-top: 0; padding-bottom: 0"
						onclick={(e) => {
							e.stopPropagation()
							applyRowPointer(item.absolute, e)
						}}
						oncontextmenu={(e) => openRowMenu(item.absolute, e)}
					>
						{item.absolute + 1}
					</td>
					{#each item.cells as cell, ci (ci)}
						{@const isNull = cell === null || cell === undefined}
						{@const text = cellDisplay(cell)}
						{@const isFocused =
							selectedCell?.row === item.absolute && selectedCell.col === ci}
						{@const cellSelected = isCellSelected(item.absolute, ci)}
						<td
							data-result-cell
							data-result-col={ci}
							role="gridcell"
							aria-selected={isFocused || cellSelected}
							class={cn(
								'max-w-xs cursor-pointer truncate border-b border-line/60 px-2.5 align-middle font-mono text-label select-none',
								isNull ? 'italic text-fg-muted' : 'text-fg',
								isFocused && 'bg-selected ring-2 ring-inset ring-accent',
								cellSelected && !isFocused && 'bg-selected',
								!isFocused && !cellSelected && !rowSelected && 'hover:bg-hover',
							)}
							style="height: {ROW_H}px; max-height: {ROW_H}px; padding-top: 0; padding-bottom: 0"
							title={text}
							onclick={(e) => {
								e.stopPropagation()
								applyCellPointer(item.absolute, ci, e)
							}}
							ondblclick={(e) => {
								e.stopPropagation()
								openCellModal(item.absolute, ci)
							}}
							oncontextmenu={(e) => openCellMenu(item.absolute, ci, e)}
						>
							{text}
						</td>
					{/each}
				</tr>
			{/each}

			{#if windowRange.padBottom > 0}
				<tr aria-hidden="true" class="pointer-events-none">
					<td
						colspan={colCount}
						class="border-0 p-0"
						style="height: {windowRange.padBottom}px; line-height: 0; font-size: 0"
					>
						<div style="height: {windowRange.padBottom}px"></div>
					</td>
				</tr>
			{/if}
		</tbody>
	</table>
</div>

{#if cellModal}
	<CellValueModal
		column={modalColumn}
		columnType={modalType}
		rowIndex={cellModal.row}
		initialText={modalValue}
		readonly
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
