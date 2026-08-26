<script lang="ts">
	/**
	 * Windowed results grid.
	 * The mounted row window only moves when the viewport nears its edge.
	 * Everyday scrolling is native compositing of already-painted rows, so
	 * the pane does not go blank while Svelte mounts the next slice.
	 */
	import type { QueryResult } from '$lib/api/engines'
	import { ContextMenu, type ContextMenuItem } from '$lib/components/core'
	import { isMacPlatform, labelForKey } from '$lib/data/appShortcuts'
	import { appPrefs } from '$lib/stores/app-prefs.svelte'
	import {
		buildCsv,
		buildJson,
		buildMarkdown,
		buildTsv,
		copyTextToClipboard,
	} from '$lib/utils/exportTable'
	import { onMount, untrack } from 'svelte'
	import CellValueModal from '../grid/CellValueModal.svelte'
	import { valueToEditorText } from '../grid/cellView'
	import {
		RESULT_HEADER_H,
		cellDisplay,
		computeResultWindow,
		measureColLayout,
	} from './queryResultsVirtual'

	/** Virtualization row height — follows density pref / `--grid-row-height`. */
	const ROW_H = $derived(appPrefs.gridRowHeightPx)
	const HEADER_H = RESULT_HEADER_H

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
	/** First mounted local row (inclusive). */
	let winStart = $state(0)
	/** Last mounted local row (exclusive). Seeded so the first paint is not empty. */
	let winEnd = $state(128)
	/** +1 scrolling down, -1 scrolling up. Not $state — must not retrigger effects. */
	let scrollDir: 1 | -1 = 1
	let lastTop = 0
	/**
	 * Cached viewport height. Read from the ResizeObserver, never from the
	 * scroll handler: `clientHeight` there forces a synchronous layout of the
	 * rows mounted by the previous scroll event, which is a long frame right
	 * in the middle of the gesture.
	 */
	let viewH = 360
	/** Movement against `scrollDir` needed before the buffer flips sides. */
	let dirAccum = 0

	const totalRows = $derived(rows.length)
	const exportCols = $derived(columns.map((c) => ({ name: c.name })))
	const colLayout = $derived.by(() =>
		measureColLayout(columns, rows, pageStart),
	)
	const windowIndices = $derived.by(() => {
		const start = Math.min(winStart, totalRows)
		const end = Math.min(winEnd, totalRows)
		const n = Math.max(0, end - start)
		const out = new Array<number>(n)
		for (let i = 0; i < n; i++) out[i] = start + i
		return out
	})
	const bodyH = $derived(totalRows * ROW_H)
	const innerH = $derived(HEADER_H + bodyH)
	const windowY = $derived(winStart * ROW_H)

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

	function applyWindow(scrollTop: number, dir: 1 | -1) {
		const next = computeResultWindow({
			totalRows,
			scrollTop,
			viewportH: viewH,
			rowH: ROW_H,
			headerH: HEADER_H,
			scrollDir: dir,
			current: { start: winStart, end: winEnd },
		})
		if (next.start !== winStart) winStart = next.start
		if (next.end !== winEnd) winEnd = next.end
	}

	function onScroll() {
		const el = scrollEl
		if (!el) return
		const top = el.scrollTop
		const delta = top - lastTop
		lastTop = top
		if (delta !== 0) {
			if (delta > 0 === scrollDir > 0) {
				dirAccum = 0
			} else {
				// Trackpads emit small counter-deltas mid-fling; flipping the
				// buffer on those would remount the window for nothing.
				dirAccum += Math.abs(delta)
				if (dirAccum > ROW_H * 3) {
					scrollDir = delta > 0 ? 1 : -1
					dirAccum = 0
				}
			}
		}
		// Apply in the scroll event — not rAF — so the next compositor frame
		// already has rows for the new position.
		applyWindow(top, scrollDir)
	}

	$effect(() => {
		const el = scrollEl
		if (!el) return
		el.addEventListener('scroll', onScroll, { passive: true })
		const measure = () => {
			viewH = el.clientHeight
			untrack(() => applyWindow(el.scrollTop, scrollDir))
		}
		untrack(measure)
		const ro =
			typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(measure)
		ro?.observe(el)
		return () => {
			el.removeEventListener('scroll', onScroll)
			ro?.disconnect()
		}
	})

	// Reset window when page / dataset changes
	$effect(() => {
		void pageStart
		void totalRows
		void columns.length
		void rows
		untrack(() => {
			scrollDir = 1
			lastTop = 0
			clearAllSelection()
			cellMenu = null
			rowMenu = null
			cellModal = null
			const el = scrollEl
			if (el) {
				el.scrollTop = 0
				viewH = el.clientHeight
			}
			dirAccum = 0
			applyWindow(0, 1)
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
		if (hit.kind === 'index') applyRowPointer(hit.row, e)
		else applyCellPointer(hit.row, hit.col, e)
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
		viewH = el.clientHeight
		const viewSpan = Math.max(0, viewH - HEADER_H)
		const viewBottom = viewTop + viewSpan
		if (top < viewTop) el.scrollTop = top
		else if (bottom > viewBottom) el.scrollTop = Math.max(0, bottom - viewSpan)
		lastTop = el.scrollTop
		applyWindow(el.scrollTop, scrollDir)

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
		kind: 'copy' | 'csv' | 'json' | 'markdown',
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
		} else if (kind === 'csv') {
			text = buildCsv(exportCols, rowsOut)
			okMsg = n === 1 ? 'Copied as CSV' : `Copied ${n} rows as CSV`
		} else if (kind === 'json') {
			text = buildJson(exportCols, rowsOut, true)
			okMsg = n === 1 ? 'Copied as JSON' : `Copied ${n} rows as JSON`
		} else {
			text = buildMarkdown(exportCols, rowsOut)
			okMsg = n === 1 ? 'Copied as Markdown' : `Copied ${n} rows as Markdown`
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
		const many = selKind === 'cells' && selectedCells.size > 1
		return [
			{
				id: 'view',
				label: 'View in modal',
				shortcut: '⇧Enter',
				disabled: many,
			},
			{
				id: 'copy',
				label: 'Copy',
				shortcut: copyShortcut,
				separatorBefore: true,
			},
			{ id: 'copy-json', label: 'Copy as JSON' },
			{ id: 'copy-tsv', label: 'Copy as TSV' },
			{
				id: 'copy-row',
				label: 'Copy row',
				separatorBefore: true,
			},
			{ id: 'copy-row-csv', label: 'Copy row as CSV' },
			{ id: 'copy-row-json', label: 'Copy row as JSON' },
			{ id: 'copy-row-markdown', label: 'Copy row as Markdown' },
			{
				id: 'copy-column',
				label: 'Copy column name',
				separatorBefore: true,
			},
		]
	})

	const rowMenuItems = $derived.by((): ContextMenuItem[] => [
		{
			id: 'copy',
			label: 'Copy',
			shortcut: copyShortcut,
		},
		{ id: 'copy-csv', label: 'Copy as CSV' },
		{ id: 'copy-json', label: 'Copy as JSON' },
		{ id: 'copy-markdown', label: 'Copy as Markdown' },
	])

	function onCellMenuPick(id: string) {
		const ctx = cellMenu
		cellMenu = null
		if (!ctx) return
		if (id === 'view') {
			openCellModal(ctx.row, ctx.col)
			return
		}
		if (id === 'copy') void copyCellSelection('copy')
		if (id === 'copy-json') void copyCellSelection('json')
		if (id === 'copy-tsv') void copyCellSelection('tsv')
		if (id === 'copy-row') void copyRowAs([ctx.row], 'copy')
		if (id === 'copy-row-csv') void copyRowAs([ctx.row], 'csv')
		if (id === 'copy-row-json') void copyRowAs([ctx.row], 'json')
		if (id === 'copy-row-markdown') void copyRowAs([ctx.row], 'markdown')
		if (id === 'copy-column') void copyColumnName(ctx.col)
	}

	function onRowMenuPick(id: string) {
		const ctx = rowMenu
		rowMenu = null
		if (!ctx) return
		const targets = targetRows(ctx.row)
		if (id === 'copy') void copyRowAs(targets, 'copy')
		if (id === 'copy-csv') void copyRowAs(targets, 'csv')
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
		<div class="qrg-head bg-elevated" role="row" style="height: {HEADER_H}px">
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
					title={col.dataType ?? undefined}
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
						aria-selected={isRowSelected(absolute)}
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
							{@const text = cellDisplay(cell)}
							{@const isFocused =
								selectedCell?.row === absolute && selectedCell.col === ci}
							<div
								data-result-cell
								data-result-col={ci}
								data-result-row={absolute}
								data-null={cell === null || cell === undefined ? '' : undefined}
								data-focus={isFocused ? '' : undefined}
								role="gridcell"
								aria-selected={isFocused || isCellSelected(absolute, ci)}
								class="qrg-cell qrg-data border-b border-line/60 px-2.5 font-mono text-label"
								title={text}
							>
								{text}
							</div>
						{/each}
					</div>
				{/each}
			</div>
		</div>
	</div>
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
