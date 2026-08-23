<script lang="ts">
	import type {
		TablePreviewFilter,
		TablePreviewSortDir,
	} from '$lib/api/engines'
	import { persistTableEdits } from '$lib/api/tablePersist'
	import { ContextMenu, type ContextMenuItem } from '$lib/components/core'
	import { appPrefs } from '$lib/stores/app-prefs.svelte'
	import type { ColumnReference, TableMeta } from '$lib/types/workspace'
	import {
		selectDraftToValue,
		usesSelectEditor,
		valueToSelectDraft,
	} from '$lib/utils/columnEditor'
	import {
		buildCsv,
		buildJson,
		buildMarkdown,
		buildTsv,
		copyTextToClipboard,
		objectsToRows,
	} from '$lib/utils/exportTable'
	import { onMount } from 'svelte'
	import CellValueModal from './grid/CellValueModal.svelte'
	import { valueToEditorText } from './grid/cellView'
	import {
		cellText,
		type FilterMode,
		type FilterOp,
		type FilterRule,
		looksLikeJsonFilter,
		newRule,
		opNeedsValue,
		parseCodeFilters,
		parseDraft,
		rulesToCode,
		valuesEqual,
	} from './grid/filterLogic'
	import GridFilterBar from './grid/GridFilterBar.svelte'
	import GridStatusBar from './grid/GridStatusBar.svelte'
	import GridTable, { type CellRef } from './grid/GridTable.svelte'
	import { isSessionMissingError } from './shell/sessionUtils'

	interface Props {
		table: TableMeta
		selectedRowIndex: number | null
		onselectrow: (index: number) => void
		rowSyncTick?: number
		onbaselinechange?: () => void
		/** Notify parent when unsaved edit count changes (for tab dirty indicator). */
		ondirtychange?: (dirty: boolean) => void
		/** Last saved row snapshots — bound so the inspector can mark dirty fields. */
		originals?: Record<string, unknown>[]
		engine?: string
		sessionId?: string | null
		profileReadOnly?: boolean
		/** Reload live rows from the engine (Refresh, pager, save). */
		onreload?: (opts?: {
			offset?: number
			filters?: TablePreviewFilter[]
			codeFilter?: string
			sortColumn?: string
			sortDir?: TablePreviewSortDir
		}) => Promise<void>
		/** Re-open a dead engine session; returns the new session id. */
		onreconnect?: () => Promise<string | null>
		/**
		 * When opening a table via FK jump, seed the filter bar so pager/refresh
		 * keep the same predicate the plugin already applied.
		 */
		seedFilters?: TablePreviewFilter[] | null
		/** Raw code-filter text so remounts stay in code mode. */
		seedCodeFilter?: string | null
		/** Seeded preview sort when the grid remounts (filter apply / FK jump). */
		seedSort?: { column: string; dir: TablePreviewSortDir } | null
		/** Plugin-owned placeholder for the code-filter box. */
		codeFilterPlaceholder?: string
		/** Plugin-owned FK jump from a cell. */
		onopenreference?: (ref: ColumnReference, value: unknown) => void
	}

	let {
		table,
		selectedRowIndex,
		onselectrow,
		rowSyncTick = 0,
		onbaselinechange,
		ondirtychange,
		originals = $bindable<Record<string, unknown>[]>([]),
		engine = '',
		sessionId = null,
		profileReadOnly = false,
		onreload,
		onreconnect,
		seedFilters = null,
		seedCodeFilter = null,
		seedSort = null,
		codeFilterPlaceholder = '{ "email": "user@example.com" }',
		onopenreference,
	}: Props = $props()

	let seed = $state<Record<string, unknown>[]>([])
	let rows = $state<Record<string, unknown>[]>([])
	let dirtyKeys = $state<Set<string>>(new Set())
	/** Absolute indices of rows added locally (not yet saved). */
	let newRowIndices = $state<Set<number>>(new Set())
	/** Absolute indices of persisted rows marked for delete (not yet saved). */
	let deletedIndices = $state<Set<number>>(new Set())
	let rowMenu = $state<{ row: number; x: number; y: number } | null>(null)
	let cellMenu = $state<{
		row: number
		col: string
		x: number
		y: number
	} | null>(null)
	let editing = $state<CellRef | null>(null)
	/** Focused cell for keyboard nav / copy-paste (absolute row index). */
	let selectedCell = $state<CellRef | null>(null)
	/** Local row highlight — inspector only opens via Show details. */
	let highlightRow = $state<number | null>(null)
	/** Full-row multi-select (row-number clicks). */
	let selectedRows = $state<Set<number>>(new Set())
	/** Shift-range starts here (row mode). */
	let selectionAnchor = $state<number | null>(null)
	/** Cell multi-select (`row::col`). Independent of full-row mode. */
	let selectedCells = $state<Set<string>>(new Set())
	/** Shift-range starts here (cell mode). */
	let cellAnchor = $state<CellRef | null>(null)
	let selKind = $state<'none' | 'rows' | 'cells'>('none')
	let draft = $state('')
	let inputEl = $state<HTMLInputElement | HTMLSelectElement | null>(null)
	let gridShellEl = $state<HTMLDivElement | null>(null)
	let cellModal = $state<{
		row: number
		col: string
		type: string
		text: string
	} | null>(null)

	const PAGE_SIZE = 100
	let page = $state(0)
	let refreshing = $state(false)
	let saving = $state(false)
	let statusMsg = $state<string | null>(null)

	let loadedTableId = $state<string | null>(null)
	let appliedSyncTick = $state(0)

	let filterMode = $state<FilterMode>('ui')
	let filterRules = $state<FilterRule[]>([])
	let filterCode = $state('')
	let filtersActive = $state(false)
	/** Single-column preview sort. Switching columns replaces the previous order. */
	let sortColumn = $state<string | null>(null)
	let sortDir = $state<TablePreviewSortDir | null>(null)

	const columnOptions = $derived(
		table.columns.map((c) => ({ value: c.name, label: c.name })),
	)

	const tablePath = $derived(
		table.schema
			? `${table.database}.${table.schema}.${table.name}`
			: `${table.database}.${table.name}`,
	)

	/** Unsaved cell edits + newly added rows + pending deletes. */
	const dirtyCount = $derived.by(() => {
		let inserts = 0
		for (const i of newRowIndices) {
			if (!deletedIndices.has(i)) inserts += 1
		}
		let deletes = 0
		for (const i of deletedIndices) {
			if (!newRowIndices.has(i)) deletes += 1
		}
		return dirtyKeys.size + inserts + deletes
	})

	$effect(() => {
		ondirtychange?.(dirtyCount > 0)
	})

	function makeRule(): FilterRule {
		return newRule(table.columns[0]?.name ?? '')
	}

	function activeRules(): FilterRule[] {
		if (!filtersActive) return []
		if (filterMode === 'code') {
			return parseCodeFilters(filterCode) ?? []
		}
		return filterRules.filter((r) => {
			if (!r.column) return false
			if (!opNeedsValue(r.op)) return true
			return r.value.trim() !== ''
		})
	}

	const hasActiveFilters = $derived(
		filtersActive &&
			(filterMode === 'code'
				? filterCode.trim().length > 0
				: activeRules().length > 0),
	)

	function previewFilters(): TablePreviewFilter[] {
		if (!filtersActive || filterMode === 'code') return []
		return activeRules().map((r) => ({
			column: r.column,
			op: r.op,
			value: r.value,
		}))
	}

	function reloadOpts(offset: number) {
		const opts: {
			offset: number
			filters: TablePreviewFilter[]
			codeFilter?: string
			sortColumn?: string
			sortDir?: TablePreviewSortDir
		} = {
			offset,
			filters: previewFilters(),
		}
		if (filtersActive && filterMode === 'code' && filterCode.trim()) {
			opts.codeFilter = filterCode.trim()
		}
		if (sortColumn && sortDir) {
			opts.sortColumn = sortColumn
			opts.sortDir = sortDir
		}
		return opts
	}

	function cycleSort(column: string) {
		if (dirtyCount > 0) {
			flashStatus('Save or reset changes before sorting.')
			return
		}
		if (sortColumn === column) {
			if (sortDir === 'asc') {
				sortDir = 'desc'
			} else {
				sortColumn = null
				sortDir = null
			}
		} else {
			sortColumn = column
			sortDir = 'asc'
		}
		page = 0
		void reloadFromEngine(0)
	}

	/** Current engine page (already filtered server-side). */
	const pageEntries = $derived(rows.map((row, index) => ({ row, index })))
	const filteredCount = $derived(rows.length)
	const serverTotal = $derived(Math.max(table.rowCount, rows.length))
	const totalPages = $derived(
		Math.max(1, Math.ceil(serverTotal / PAGE_SIZE) || 1),
	)
	const safePage = $derived(Math.min(page, totalPages - 1))
	const pageStart = $derived(safePage * PAGE_SIZE)
	const pageEnd = $derived(pageStart + filteredCount)

	function setFilterMode(mode: FilterMode) {
		if (mode === filterMode) return
		if (mode === 'code') {
			filterCode = rulesToCode(filterRules)
		} else {
			if (looksLikeJsonFilter(filterCode)) {
				if (filterRules.length === 0) filterRules = [makeRule()]
			} else {
				const parsed = parseCodeFilters(filterCode)
				if (parsed && parsed.length > 0) {
					filterRules = parsed
				} else if (filterRules.length === 0) {
					filterRules = [makeRule()]
				}
			}
		}
		filterMode = mode
	}

	async function reloadFromEngine(offset: number) {
		if (!onreload) return
		refreshing = true
		statusMsg = null
		try {
			await onreload(reloadOpts(offset))
			loadFromTable(true)
			appliedSyncTick = rowSyncTick
		} catch (err) {
			statusMsg = err instanceof Error ? err.message : String(err)
		} finally {
			refreshing = false
		}
	}

	function applyFilters() {
		if (dirtyCount > 0) {
			flashStatus('Save or reset changes before filtering.')
			return
		}
		if (filterMode === 'code' && !filterCode.trim()) {
			flashStatus('Enter a filter')
			return
		}
		filtersActive = true
		page = 0
		void reloadFromEngine(0)
	}

	function clearFilters() {
		if (dirtyCount > 0) {
			flashStatus('Save or reset changes before filtering.')
			return
		}
		filtersActive = false
		filterRules = [makeRule()]
		filterCode = ''
		page = 0
		void reloadFromEngine(0)
	}

	function addRule() {
		filterRules = [...filterRules, makeRule()]
	}

	function removeRule(id: string) {
		filterRules = filterRules.filter((r) => r.id !== id)
		if (filterRules.length === 0) filterRules = [makeRule()]
	}

	function updateRule(id: string, patch: Partial<FilterRule>) {
		filterRules = filterRules.map((r) => (r.id === id ? { ...r, ...patch } : r))
	}

	function updateFirstRule(patch: Partial<FilterRule>) {
		if (filterRules.length === 0) {
			filterRules = [{ ...makeRule(), ...patch }]
			return
		}
		updateRule(filterRules[0].id, patch)
	}

	function cellKey(row: number, col: string): string {
		return `${row}::${col}`
	}

	function isDirty(row: number, col: string): boolean {
		return dirtyKeys.has(cellKey(row, col))
	}

	function recomputeDirty(
		nextRows: Record<string, unknown>[],
		base: Record<string, unknown>[],
	) {
		const next = new Set<string>()
		for (let i = 0; i < nextRows.length; i++) {
			for (const col of table.columns) {
				if (!valuesEqual(nextRows[i]?.[col.name], base[i]?.[col.name])) {
					next.add(cellKey(i, col.name))
				}
			}
		}
		dirtyKeys = next
	}

	function loadFromTable(resetBaseline: boolean) {
		const cloned = table.rows.map((r) => ({ ...r }))
		rows = cloned.map((r) => ({ ...r }))
		// Only adopt incoming rows as the saved baseline after load / save /
		// reset. Inspector (and other in-memory) writes reuse this path with
		// resetBaseline=false — those must stay dirty until the user saves.
		if (resetBaseline || originals.length === 0) {
			seed = cloned.map((r) => ({ ...r }))
			originals = cloned.map((r) => ({ ...r }))
			dirtyKeys = new Set()
			newRowIndices = new Set()
			deletedIndices = new Set()
		} else {
			recomputeDirty(cloned, originals)
		}
		editing = null
		draft = ''
		loadedTableId = table.id
		if (resetBaseline) {
			selectedRows = new Set()
			selectionAnchor = null
			selectedCells = new Set()
			cellAnchor = null
			selKind = 'none'
			highlightRow = null
		}
	}

	function blankRow(): Record<string, unknown> {
		const row: Record<string, unknown> = {}
		for (const col of table.columns) {
			row[col.name] = null
		}
		return row
	}

	/** Shift absolute indices after inserting a row at the front. */
	function shiftIndicesAfterPrepend() {
		const shiftSet = (set: Set<number>) => new Set([...set].map((i) => i + 1))
		newRowIndices = shiftSet(newRowIndices)
		deletedIndices = shiftSet(deletedIndices)

		const nextDirty = new Set<string>()
		for (const key of dirtyKeys) {
			const sep = key.indexOf('::')
			if (sep < 0) continue
			const i = Number(key.slice(0, sep))
			const col = key.slice(sep + 2)
			if (!Number.isFinite(i) || !col) continue
			nextDirty.add(cellKey(i + 1, col))
		}
		dirtyKeys = nextDirty

		if (editing) {
			editing = { row: editing.row + 1, col: editing.col }
		}
		if (selectedCell) {
			selectedCell = { row: selectedCell.row + 1, col: selectedCell.col }
		}
		if (highlightRow != null) {
			highlightRow = highlightRow + 1
		}
		selectedRows = new Set([...selectedRows].map((i) => i + 1))
		if (selectionAnchor != null) selectionAnchor = selectionAnchor + 1
		const nextCells = new Set<string>()
		for (const key of selectedCells) {
			const sep = key.indexOf('::')
			if (sep < 0) continue
			const i = Number(key.slice(0, sep))
			const col = key.slice(sep + 2)
			if (Number.isFinite(i)) nextCells.add(selCellKey(i + 1, col))
		}
		selectedCells = nextCells
		if (cellAnchor)
			cellAnchor = { row: cellAnchor.row + 1, col: cellAnchor.col }
	}

	/**
	 * Insert an unsaved local row at the top of the grid.
	 * Baseline equals the inserted values so only real edits dirty cells;
	 * membership in newRowIndices keeps Save/Reset visible.
	 */
	function insertLocalRowAtTop(row: Record<string, unknown>) {
		shiftIndicesAfterPrepend()
		const copy = { ...row }
		rows = [copy, ...rows]
		table.rows.unshift({ ...copy })
		originals = [{ ...copy }, ...originals]
		seed = [{ ...copy }, ...seed]
		newRowIndices = new Set([0, ...newRowIndices])

		const firstCol = table.columns[0]?.name
		if (firstCol) {
			selectedCell = { row: 0, col: firstCol }
			highlightRow = 0
			if (shouldOpenInspector()) onselectrow(0)
			requestAnimationFrame(() => {
				scrollCellIntoView(0, firstCol)
				focusGridShell()
			})
		}
		return 0
	}

	function addNewRow() {
		if (profileReadOnly) {
			flashStatus('Connection is read-only')
			return
		}
		if (editing) commitEdit()
		insertLocalRowAtTop(blankRow())
		statusMsg = 'Row added'
		setTimeout(() => {
			if (statusMsg === 'Row added') statusMsg = null
		}, 1500)
	}

	function cloneFocusedRow() {
		const targets = actionRows()
		if (targets.length === 0) {
			flashStatus('Select a row to duplicate')
			return
		}
		cloneRows(targets)
	}

	function flashStatus(msg: string, ms = 1600) {
		statusMsg = msg
		setTimeout(() => {
			if (statusMsg === msg) statusMsg = null
		}, ms)
	}

	function selectedRowList(): number[] {
		return [...selectedRows].sort((a, b) => a - b)
	}

	function selCellKey(row: number, col: string): string {
		return `${row}::${col}`
	}

	function rowsFromSelectedCells(): number[] {
		const next = new Set<number>()
		for (const key of selectedCells) {
			const sep = key.indexOf('::')
			if (sep < 0) continue
			const i = Number(key.slice(0, sep))
			if (Number.isFinite(i)) next.add(i)
		}
		return [...next].sort((a, b) => a - b)
	}

	function actionRows(): number[] {
		if (selKind === 'rows' && selectedRows.size > 0) return selectedRowList()
		if (selKind === 'cells' && selectedCells.size > 0) {
			return rowsFromSelectedCells()
		}
		if (highlightRow != null) return [highlightRow]
		if (selectedCell) return [selectedCell.row]
		if (selectedRowIndex != null) return [selectedRowIndex]
		return []
	}

	function actionCells(): { row: number; col: string }[] {
		if (selKind === 'cells' && selectedCells.size > 0) {
			const out: { row: number; col: string }[] = []
			for (const key of selectedCells) {
				const sep = key.indexOf('::')
				if (sep < 0) continue
				const row = Number(key.slice(0, sep))
				const col = key.slice(sep + 2)
				if (Number.isFinite(row) && col) out.push({ row, col })
			}
			return out
		}
		if (selKind === 'rows' && selectedRows.size > 0) {
			const cols = colNames()
			const out: { row: number; col: string }[] = []
			for (const row of selectedRowList()) {
				for (const col of cols) out.push({ row, col })
			}
			return out
		}
		if (selectedCell) return [{ row: selectedCell.row, col: selectedCell.col }]
		return []
	}

	function setCellsNull(cells: { row: number; col: string }[]) {
		if (profileReadOnly) {
			flashStatus('Connection is read-only')
			return
		}
		if (editing) commitEdit()
		let n = 0
		for (const { row, col } of cells) {
			if (deletedIndices.has(row)) continue
			const cur = rows[row]?.[col]
			if (cur === null || cur === undefined) continue
			applyCellValue(row, col, 'NULL')
			n += 1
		}
		if (n === 0) return
		flashStatus(n === 1 ? 'Set to NULL' : `Set ${n} cells to NULL`)
	}

	function targetRows(primary: number): number[] {
		const all = actionRows()
		if (all.includes(primary) && all.length > 1) return all
		return [primary]
	}

	function clearCellSelection() {
		selectedCells = new Set()
		cellAnchor = null
	}

	function clearRowSelection() {
		selectedRows = new Set()
		selectionAnchor = null
	}

	function setExclusiveRow(row: number) {
		selKind = 'rows'
		clearCellSelection()
		selectedCell = null
		selectedRows = new Set([row])
		selectionAnchor = row
		highlightRow = row
	}

	function toggleRow(row: number) {
		selKind = 'rows'
		clearCellSelection()
		selectedCell = null
		const next = new Set(selectedRows)
		if (next.has(row)) next.delete(row)
		else next.add(row)
		if (next.size === 0) next.add(row)
		selectedRows = next
		selectionAnchor = row
		highlightRow = row
	}

	function selectRowRange(to: number) {
		selKind = 'rows'
		clearCellSelection()
		selectedCell = null
		const from = selectionAnchor ?? highlightRow ?? to
		const lo = Math.min(from, to)
		const hi = Math.max(from, to)
		const next = new Set<number>()
		for (let i = lo; i <= hi; i++) next.add(i)
		selectedRows = next
		highlightRow = to
	}

	function applyRowPointer(row: number, e?: MouseEvent) {
		const mod = !!(e && (e.metaKey || e.ctrlKey))
		const shift = !!(e && e.shiftKey)
		if (shift) selectRowRange(row)
		else if (mod) toggleRow(row)
		else setExclusiveRow(row)
	}

	function setExclusiveCell(row: number, col: string) {
		selKind = 'cells'
		clearRowSelection()
		selectedCell = { row, col }
		cellAnchor = { row, col }
		selectedCells = new Set([selCellKey(row, col)])
		highlightRow = row
	}

	function toggleCell(row: number, col: string) {
		selKind = 'cells'
		clearRowSelection()
		const key = selCellKey(row, col)
		const next = new Set(selectedCells)
		if (next.has(key)) next.delete(key)
		else next.add(key)
		if (next.size === 0) next.add(key)
		selectedCells = next
		selectedCell = { row, col }
		cellAnchor = { row, col }
		highlightRow = row
	}

	function selectCellRect(toRow: number, toCol: string) {
		selKind = 'cells'
		clearRowSelection()
		const cols = colNames()
		const from = cellAnchor ?? selectedCell ?? { row: toRow, col: toCol }
		const fromC = cols.indexOf(from.col)
		const toC = cols.indexOf(toCol)
		const r0 = Math.min(from.row, toRow)
		const r1 = Math.max(from.row, toRow)
		const c0 = Math.min(fromC < 0 ? 0 : fromC, toC < 0 ? 0 : toC)
		const c1 = Math.max(fromC < 0 ? 0 : fromC, toC < 0 ? 0 : toC)
		const next = new Set<string>()
		for (let r = r0; r <= r1; r++) {
			for (let c = c0; c <= c1; c++) {
				const name = cols[c]
				if (name) next.add(selCellKey(r, name))
			}
		}
		selectedCells = next
		selectedCell = { row: toRow, col: toCol }
		highlightRow = toRow
	}

	function applyCellPointer(row: number, col: string, e?: MouseEvent) {
		const mod = !!(e && (e.metaKey || e.ctrlKey))
		const shift = !!(e && e.shiftKey)
		if (shift) selectCellRect(row, col)
		else if (mod) toggleCell(row, col)
		else setExclusiveCell(row, col)
	}

	function cellSelectionPayload(): {
		columns: { name: string }[]
		cells: unknown[][]
		count: number
	} | null {
		if (selKind !== 'cells' || selectedCells.size === 0) return null
		const cols = colNames()
		const parsed: { row: number; ci: number }[] = []
		for (const key of selectedCells) {
			const sep = key.indexOf('::')
			if (sep < 0) continue
			const row = Number(key.slice(0, sep))
			const col = key.slice(sep + 2)
			const ci = cols.indexOf(col)
			if (!Number.isFinite(row) || ci < 0) continue
			parsed.push({ row, ci })
		}
		if (parsed.length === 0) return null
		const rowIdx = [...new Set(parsed.map((p) => p.row))].sort((a, b) => a - b)
		const colIdx = [...new Set(parsed.map((p) => p.ci))].sort((a, b) => a - b)
		const columns = colIdx.map((i) => ({ name: cols[i]! }))
		const cells = rowIdx.map((r) =>
			colIdx.map((c) => {
				const name = cols[c]!
				return selectedCells.has(selCellKey(r, name))
					? (rows[r]?.[name] ?? '')
					: ''
			}),
		)
		return { columns, cells, count: parsed.length }
	}

	function rowExportPayload(indices: number[]) {
		const columns = table.columns.map((c) => ({ name: c.name }))
		const objects = indices
			.map((i) => rows[i])
			.filter((r): r is Record<string, unknown> => !!r)
		const cells = objectsToRows(columns, objects)
		return { columns, cells, count: objects.length }
	}

	async function copyRowAs(
		indices: number[],
		kind: 'copy' | 'csv' | 'markdown' | 'json',
	) {
		const { columns, cells, count } = rowExportPayload(indices)
		if (count === 0) return
		let text = ''
		let okMsg = 'Copied'
		const n = count === 1 ? 'row' : `${count} rows`
		switch (kind) {
			case 'copy':
				text = buildTsv(columns, cells)
				okMsg = count === 1 ? 'Row copied' : `${count} rows copied`
				break
			case 'csv':
				text = buildCsv(columns, cells)
				okMsg = `Copied ${n} as CSV`
				break
			case 'markdown':
				text = buildMarkdown(columns, cells)
				okMsg = `Copied ${n} as Markdown`
				break
			case 'json':
				text = buildJson(columns, cells, true)
				okMsg = `Copied ${n} as JSON`
				break
		}
		const res = await copyTextToClipboard(text)
		flashStatus(res.ok ? okMsg : res.cancelled ? 'Copy cancelled' : res.error)
	}

	async function copySelection(
		kind: 'copy' | 'csv' | 'markdown' | 'json',
		fallbackRows?: number[],
	) {
		if (selKind === 'cells' && selectedCells.size > 1) {
			const payload = cellSelectionPayload()
			if (payload) {
				let text = ''
				let okMsg = 'Copied'
				const n = payload.count === 1 ? 'cell' : `${payload.count} cells`
				switch (kind) {
					case 'copy':
						text = buildTsv(payload.columns, payload.cells)
						okMsg = `${payload.count} cells copied`
						break
					case 'csv':
						text = buildCsv(payload.columns, payload.cells)
						okMsg = `Copied ${n} as CSV`
						break
					case 'markdown':
						text = buildMarkdown(payload.columns, payload.cells)
						okMsg = `Copied ${n} as Markdown`
						break
					case 'json':
						text = buildJson(payload.columns, payload.cells, true)
						okMsg = `Copied ${n} as JSON`
						break
				}
				const res = await copyTextToClipboard(text)
				flashStatus(
					res.ok ? okMsg : res.cancelled ? 'Copy cancelled' : res.error,
				)
				return
			}
		}
		await copyRowAs(fallbackRows ?? actionRows(), kind)
	}

	function snapshotClone(index: number): Record<string, unknown> | null {
		const src = rows[index]
		if (!src) return null
		const row: Record<string, unknown> = { ...src }
		for (const col of table.columns) {
			if (
				col.pk ||
				col.name === 'id' ||
				col.name === '_id' ||
				col.name === 'rowid'
			) {
				row[col.name] = null
			}
		}
		return row
	}

	function cloneRows(indices: number[]) {
		if (profileReadOnly) {
			flashStatus('Connection is read-only')
			return
		}
		if (editing) commitEdit()
		const snaps = [...new Set(indices)]
			.sort((a, b) => a - b)
			.map((i) => snapshotClone(i))
			.filter((r): r is Record<string, unknown> => !!r)
		if (snaps.length === 0) return
		for (let i = snaps.length - 1; i >= 0; i--) {
			insertLocalRowAtTop(snaps[i]!)
		}
		flashStatus(
			snaps.length === 1
				? 'Row duplicated — save to insert'
				: `${snaps.length} rows duplicated — save to insert`,
		)
	}

	function cloneRow(index: number) {
		cloneRows([index])
	}

	function remapAfterRemove(index: number, i: number): number | null {
		if (i === index) return null
		return i > index ? i - 1 : i
	}

	/** Drop an unsaved insert/clone from local state (no server DELETE). */
	function removeLocalRow(index: number) {
		rows = rows.filter((_, i) => i !== index)
		originals = originals.filter((_, i) => i !== index)
		seed = seed.filter((_, i) => i !== index)
		table.rows.splice(index, 1)

		const remapSet = (set: Set<number>) => {
			const next = new Set<number>()
			for (const i of set) {
				const mapped = remapAfterRemove(index, i)
				if (mapped != null) next.add(mapped)
			}
			return next
		}
		newRowIndices = remapSet(newRowIndices)
		deletedIndices = remapSet(deletedIndices)

		const nextDirty = new Set<string>()
		for (const key of dirtyKeys) {
			const sep = key.indexOf('::')
			if (sep < 0) continue
			const i = Number(key.slice(0, sep))
			const col = key.slice(sep + 2)
			if (!Number.isFinite(i) || !col) continue
			const mapped = remapAfterRemove(index, i)
			if (mapped == null) continue
			nextDirty.add(cellKey(mapped, col))
		}
		dirtyKeys = nextDirty

		if (editing?.row === index) {
			editing = null
			draft = ''
		} else if (editing && editing.row > index) {
			editing = { row: editing.row - 1, col: editing.col }
		}

		if (selectedCell?.row === index) selectedCell = null
		else if (selectedCell && selectedCell.row > index) {
			selectedCell = { row: selectedCell.row - 1, col: selectedCell.col }
		}

		if (highlightRow === index) highlightRow = null
		else if (highlightRow != null && highlightRow > index) {
			highlightRow = highlightRow - 1
		}

		const nextSel = new Set<number>()
		for (const i of selectedRows) {
			const mapped = remapAfterRemove(index, i)
			if (mapped != null) nextSel.add(mapped)
		}
		selectedRows = nextSel
		if (selectionAnchor === index) selectionAnchor = highlightRow
		else if (selectionAnchor != null && selectionAnchor > index) {
			selectionAnchor = selectionAnchor - 1
		}

		const nextCells = new Set<string>()
		for (const key of selectedCells) {
			const sep = key.indexOf('::')
			if (sep < 0) continue
			const i = Number(key.slice(0, sep))
			const col = key.slice(sep + 2)
			if (!Number.isFinite(i)) continue
			const mapped = remapAfterRemove(index, i)
			if (mapped != null) nextCells.add(selCellKey(mapped, col))
		}
		selectedCells = nextCells
		if (cellAnchor?.row === index) cellAnchor = selectedCell
		else if (cellAnchor && cellAnchor.row > index) {
			cellAnchor = { row: cellAnchor.row - 1, col: cellAnchor.col }
		}
	}

	function deleteRows(indices: number[]) {
		const unique = [...new Set(indices)].sort((a, b) => b - a)
		if (unique.length === 0) return
		let removed = 0
		let marked = 0
		for (const i of unique) {
			const beforeDel = deletedIndices.size
			const beforeRows = rows.length
			deleteRow(i, true)
			if (rows.length < beforeRows) removed += 1
			else if (deletedIndices.size > beforeDel) marked += 1
		}
		if (removed && marked) {
			flashStatus(
				`${removed} unsaved removed · ${marked} marked for delete — save to apply`,
			)
		} else if (removed) {
			flashStatus(
				removed === 1
					? 'Unsaved row removed'
					: `${removed} unsaved rows removed`,
			)
		} else if (marked) {
			flashStatus(
				marked === 1
					? 'Marked for delete — save to apply'
					: `${marked} rows marked for delete — save to apply`,
			)
		}
	}

	function undeleteRows(indices: number[]) {
		const next = new Set(deletedIndices)
		let n = 0
		for (const i of indices) {
			if (next.has(i)) {
				next.delete(i)
				n += 1
			}
		}
		if (n === 0) return
		deletedIndices = next
		flashStatus(n === 1 ? 'Delete undone' : `${n} deletes undone`)
	}

	function deleteRow(index: number, quiet = false) {
		if (profileReadOnly) {
			if (!quiet) flashStatus('Connection is read-only')
			return
		}
		if (editing) commitEdit()
		if (!rows[index]) return
		if (deletedIndices.has(index)) return

		// Unsaved insert/clone: remove locally. Staging a DELETE has no PK and
		// blocks Save (dirtyCount treats new+deleted as a no-op).
		if (newRowIndices.has(index)) {
			removeLocalRow(index)
			if (!quiet) flashStatus('Unsaved row removed')
			return
		}

		const hasPk = table.columns.some(
			(c) => c.pk || c.name === 'id' || c.name === '_id' || c.name === 'rowid',
		)
		if (!hasPk) {
			if (!quiet) flashStatus('Cannot delete: table has no primary key')
			return
		}

		deletedIndices = new Set([...deletedIndices, index])
		if (editing?.row === index) {
			editing = null
			draft = ''
		}
		if (selectedCell?.row === index) selectedCell = null
		if (!quiet) flashStatus('Marked for delete — save to apply')
	}

	function undeleteRow(index: number) {
		if (!deletedIndices.has(index)) return
		deletedIndices = new Set([...deletedIndices].filter((i) => i !== index))
		flashStatus('Delete undone')
	}

	const rowMenuItems = $derived.by((): ContextMenuItem[] => {
		const ro = profileReadOnly
		const primary = rowMenu?.row
		const targets =
			primary != null && selectedRows.has(primary) && selectedRows.size > 1
				? selectedRowList()
				: primary != null
					? [primary]
					: []
		const n = targets.length
		const pendingAll = n > 0 && targets.every((i) => deletedIndices.has(i))
		const pendingAny = targets.some((i) => deletedIndices.has(i))
		const liveAny = targets.some((i) => !deletedIndices.has(i))
		const many = n > 1
		const items: ContextMenuItem[] = [
			{ id: 'copy', label: 'Copy' },
			{ id: 'csv', label: 'Copy as CSV' },
			{ id: 'markdown', label: 'Copy as Markdown' },
			{ id: 'json', label: 'Copy as JSON' },
			{
				id: 'clone',
				label: 'Duplicate row',
				shortcut: '⌘/Ctrl+D',
				disabled: ro || pendingAll,
				separatorBefore: true,
			},
		]
		if (liveAny) {
			items.push({
				id: 'delete',
				label: 'Delete row',
				shortcut: '⌘/Ctrl+Delete',
				danger: true,
				disabled: ro,
			})
		}
		if (pendingAny) {
			items.push({
				id: 'undelete',
				label: 'Undo delete',
				disabled: ro,
			})
		}
		items.push({
			id: 'details',
			label: 'Show details',
			separatorBefore: true,
			disabled: many,
		})
		return items
	})

	function openRowMenu(row: number, e: MouseEvent) {
		cellMenu = null
		if (selKind !== 'rows' || !selectedRows.has(row)) setExclusiveRow(row)
		else highlightRow = row
		rowMenu = { row, x: e.clientX, y: e.clientY }
	}

	function openCellMenu(row: number, col: string, e: MouseEvent) {
		rowMenu = null
		if (selKind !== 'cells' || !selectedCells.has(selCellKey(row, col))) {
			setExclusiveCell(row, col)
		} else {
			selectedCell = { row, col }
			highlightRow = row
		}
		cellMenu = { row, col, x: e.clientX, y: e.clientY }
	}

	function onRowMenuPick(id: string) {
		const row = rowMenu?.row
		rowMenu = null
		if (row == null) return
		const targets = targetRows(row)
		if (id === 'copy' || id === 'csv' || id === 'markdown' || id === 'json') {
			void copySelection(id, targets)
			return
		}
		if (id === 'clone') {
			cloneRows(targets)
			return
		}
		if (id === 'delete')
			deleteRows(targets.filter((i) => !deletedIndices.has(i)))
		if (id === 'undelete') undeleteRows(targets)
		if (id === 'details') {
			highlightRow = row
			onselectrow(row)
		}
	}

	const cellMenuItems = $derived.by((): ContextMenuItem[] => {
		const ctx = cellMenu
		if (!ctx) return []
		const ro = profileReadOnly
		const targets = targetRows(ctx.row)
		const many = targets.length > 1
		const pending = deletedIndices.has(ctx.row)
		const pendingAll = targets.every((i) => deletedIndices.has(i))
		const pendingAny = targets.some((i) => deletedIndices.has(i))
		const liveAny = targets.some((i) => !deletedIndices.has(i))
		const action = actionCells()
		const cellCount = action.length
		const allNull =
			cellCount > 0 &&
			action.every((c) => {
				const v = rows[c.row]?.[c.col]
				return v === null || v === undefined
			})
		const items: ContextMenuItem[] = [
			{
				id: 'edit-modal',
				label: 'Edit Modal',
				shortcut: '⇧Enter',
				disabled: ro || pending || cellCount > 1,
			},
			{
				id: 'set-null',
				label: 'Set NULL',
				shortcut: 'Delete',
				disabled: ro || pending || allNull,
			},
			{
				id: 'copy',
				label: 'Copy',
				separatorBefore: true,
			},
			{
				id: 'paste',
				label: 'Paste',
				disabled: ro || pending,
			},
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
			{
				id: 'clone',
				label: 'Duplicate row',
				shortcut: '⌘/Ctrl+D',
				disabled: ro || pendingAll,
				separatorBefore: true,
			},
		]
		if (liveAny) {
			items.push({
				id: 'delete',
				label: 'Delete row',
				shortcut: '⌘/Ctrl+Delete',
				danger: true,
				disabled: ro,
			})
		}
		if (pendingAny) {
			items.push({
				id: 'undelete',
				label: 'Undo delete',
				disabled: ro,
			})
		}
		items.push({
			id: 'details',
			label: 'Show details',
			separatorBefore: true,
			disabled: many,
		})
		return items
	})

	async function onCellMenuPick(id: string) {
		const ctx = cellMenu
		cellMenu = null
		if (!ctx) return
		const { row, col } = ctx
		selectedCell = { row, col }
		highlightRow = row

		if (id === 'edit-modal') {
			openCellModal(row, col)
			return
		}
		if (id === 'set-null') {
			setCellsNull(actionCells())
			return
		}
		if (id === 'copy') {
			if (actionCells().length > 1) void copySelection('copy')
			else copySelectedCell()
			return
		}
		if (id === 'paste') {
			if (profileReadOnly) {
				flashStatus('Connection is read-only')
				return
			}
			if (editing) commitEdit()
			try {
				const text = await navigator.clipboard.readText()
				pasteIntoGrid(text)
			} catch {
				flashStatus('Could not read clipboard')
			}
			return
		}
		if (id === 'copy-row') {
			void copyRowAs(targetRows(row), 'copy')
			return
		}
		if (id === 'copy-row-csv') {
			void copyRowAs(targetRows(row), 'csv')
			return
		}
		if (id === 'copy-row-json') {
			void copyRowAs(targetRows(row), 'json')
			return
		}
		if (id === 'copy-row-markdown') {
			void copyRowAs(targetRows(row), 'markdown')
			return
		}
		if (id === 'copy-column') {
			const res = await copyTextToClipboard(col)
			flashStatus(res.ok ? 'Column name copied' : 'Copy failed')
			return
		}
		if (id === 'clone') {
			cloneRows(targetRows(row))
			return
		}
		if (id === 'delete') {
			deleteRows(targetRows(row).filter((i) => !deletedIndices.has(i)))
			return
		}
		if (id === 'undelete') {
			undeleteRows(targetRows(row))
			return
		}
		if (id === 'details') {
			highlightRow = row
			onselectrow(row)
		}
	}

	$effect(() => {
		const id = table.id
		if (loadedTableId === id) return
		loadFromTable(true)
		page = 0
		statusMsg = null
		// Seed filters from FK jump, or restore the code-filter box.
		const seed = seedFilters
		const codeSeed = seedCodeFilter?.trim() ?? ''
		if (codeSeed) {
			filterMode = 'code'
			filterCode = codeSeed
			filterRules = [makeRule()]
			filtersActive = true
		} else if (seed && seed.length > 0) {
			filterRules = seed.map((f) => ({
				id: `f-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
				column: f.column,
				op: (f.op as FilterOp) || 'eq',
				value: f.value ?? '',
			}))
			filterCode = rulesToCode(filterRules)
			filtersActive = true
			filterMode = 'ui'
		} else {
			filtersActive = false
			filterRules = [makeRule()]
			filterCode = ''
			filterMode = 'ui'
		}
		if (seedSort?.column && seedSort.dir) {
			sortColumn = seedSort.column
			sortDir = seedSort.dir
		} else {
			sortColumn = null
			sortDir = null
		}
		appliedSyncTick = -1
	})

	$effect(() => {
		const tick = rowSyncTick
		const tableId = table.id
		if (loadedTableId !== tableId) return
		if (tick === appliedSyncTick) return
		if (editing) return
		appliedSyncTick = tick
		loadFromTable(false)
	})

	$effect(() => {
		if (editing && inputEl) {
			inputEl.focus()
			if (inputEl instanceof HTMLInputElement) {
				inputEl.select()
			}
		}
	})

	$effect(() => {
		if (page > totalPages - 1) page = Math.max(0, totalPages - 1)
	})

	function columnDef(name: string) {
		return table.columns.find((c) => c.name === name)
	}

	function colNames(): string[] {
		return table.columns.map((c) => c.name)
	}

	function focusGridShell() {
		// Defer so we don't steal focus mid-edit open.
		queueMicrotask(() => {
			if (!editing) gridShellEl?.focus({ preventScroll: true })
		})
	}

	function shouldOpenInspector(): boolean {
		// Pref default on. If the panel is already open, always follow the row.
		return appPrefs.inspectOnRowClick || selectedRowIndex != null
	}

	function selectCell(row: number, col: string, e?: MouseEvent) {
		if (editing) {
			if (editing.row === row && editing.col === col) return
			commitEdit()
		}
		applyCellPointer(row, col, e)
		if (shouldOpenInspector()) onselectrow(row)
		focusGridShell()
		scrollCellIntoView(row, col)
	}

	function selectRowOnly(row: number, e?: MouseEvent) {
		if (editing) commitEdit()
		applyRowPointer(row, e)
		selectedCell = null
		if (shouldOpenInspector()) onselectrow(row)
		focusGridShell()
		pinScrollToRowGutter()
	}

	function gridScrollEl(): HTMLElement | null {
		return gridShellEl?.querySelector<HTMLElement>('[data-grid-scroll]') ?? null
	}

	function pinScrollToRowGutter() {
		const scroller = gridScrollEl()
		if (scroller) scroller.scrollLeft = 0
	}

	function scrollCellIntoView(row: number, col: string) {
		const root = gridShellEl
		if (!root) return
		const esc =
			typeof CSS !== 'undefined' && typeof CSS.escape === 'function'
				? CSS.escape(col)
				: col.replace(/"/g, '\\"')
		const el = root.querySelector<HTMLElement>(
			`[data-grid-cell][data-row="${row}"][data-col="${esc}"]`,
		)
		el?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
		// First data column: snap fully left so the # gutter is not left off-screen
		// after scrolling in from the right (`inline: nearest` only cares about the cell).
		if (colNames()[0] === col) pinScrollToRowGutter()
	}

	/** Current page is already fetched from the server; just scroll. */
	function ensureRowVisible(absoluteRow: number) {
		const col = selectedCell?.col ?? colNames()[0]
		if (col) scrollCellIntoView(absoluteRow, col)
	}

	function moveSelection(dRow: number, dCol: number, extend = false) {
		const cols = colNames()
		if (cols.length === 0 || pageEntries.length === 0) return

		let row =
			selectedCell?.row ??
			highlightRow ??
			selectionAnchor ??
			pageEntries[0]!.index
		let colIdx = selectedCell ? Math.max(0, cols.indexOf(selectedCell.col)) : 0

		// Map absolute row → position in the current engine page
		let filterIdx = pageEntries.findIndex((e) => e.index === row)
		if (filterIdx < 0) filterIdx = 0

		filterIdx = Math.max(0, Math.min(pageEntries.length - 1, filterIdx + dRow))
		colIdx = Math.max(0, Math.min(cols.length - 1, colIdx + dCol))

		const nextAbs = pageEntries[filterIdx]!.index
		const nextCol = cols[colIdx]!
		ensureRowVisible(nextAbs)
		highlightRow = nextAbs
		if (selKind === 'rows' || !selectedCell) {
			if (dCol !== 0 && !extend) {
				setExclusiveCell(nextAbs, nextCol)
			} else if (!extend) {
				setExclusiveRow(nextAbs)
			} else {
				selectRowRange(nextAbs)
			}
		} else if (!extend) {
			setExclusiveCell(nextAbs, nextCol)
		} else {
			selectCellRect(nextAbs, nextCol)
		}
		// Keep the row details panel in sync (same path as click selection).
		if (shouldOpenInspector()) onselectrow(nextAbs)
		// Wait a frame for page change DOM
		requestAnimationFrame(() => scrollCellIntoView(nextAbs, nextCol))
	}

	function startEdit(
		row: number,
		col: string,
		e?: MouseEvent,
		/** When set (type-to-edit), replace cell with this seed text. */
		seedDraft?: string,
	) {
		e?.stopPropagation()
		if (deletedIndices.has(row)) {
			flashStatus('Row is marked for delete — undo delete to edit')
			return
		}
		highlightRow = row
		selectedCell = { row, col }
		if (editing?.row === row && editing.col === col && seedDraft === undefined)
			return
		if (editing) commitEdit()
		const value = rows[row]?.[col]
		const def = columnDef(col)
		editing = { row, col }
		if (seedDraft !== undefined) {
			draft = seedDraft
		} else if (def && usesSelectEditor(def, value)) {
			draft = valueToSelectDraft(value)
		} else {
			draft = value === null || value === undefined ? '' : cellText(value)
		}
	}

	function applyCellValue(row: number, col: string, rawText: string) {
		const prev = rows[row]?.[col]
		const def = columnDef(col)
		const next =
			def && usesSelectEditor(def, prev)
				? selectDraftToValue(rawText, def, prev)
				: parseDraft(rawText, prev)
		rows = rows.map((r, i) => (i === row ? { ...r, [col]: next } : r))
		if (table.rows[row]) {
			table.rows[row] = { ...table.rows[row], [col]: next }
		} else {
			table.rows[row] = { ...(rows[row] ?? {}), [col]: next }
		}
		const key = cellKey(row, col)
		const original = originals[row]?.[col]
		const nextDirty = new Set(dirtyKeys)
		if (valuesEqual(next, original)) nextDirty.delete(key)
		else nextDirty.add(key)
		dirtyKeys = nextDirty
	}

	function commitEdit() {
		if (!editing) return
		const { row, col } = editing
		applyCellValue(row, col, draft)
		selectedCell = { row, col }
		editing = null
		draft = ''
		focusGridShell()
	}

	function cancelEdit() {
		if (!editing) return
		selectedCell = { row: editing.row, col: editing.col }
		editing = null
		draft = ''
		focusGridShell()
	}

	function openCellModal(row: number, col: string, seed?: string) {
		if (deletedIndices.has(row)) {
			flashStatus('Row is marked for delete — undo delete to edit')
			return
		}
		const seedText = seed !== undefined ? seed : undefined
		if (editing) {
			editing = null
			draft = ''
		}
		const value = rows[row]?.[col]
		const def = columnDef(col)
		cellModal = {
			row,
			col,
			type: def?.type ?? '',
			text: seedText !== undefined ? seedText : valueToEditorText(value),
		}
		selectedCell = { row, col }
		highlightRow = row
	}

	function applyCellModal(text: string) {
		if (!cellModal) return
		const { row, col } = cellModal
		applyCellValue(row, col, text)
		selectedCell = { row, col }
		highlightRow = row
		cellModal = null
		focusGridShell()
	}

	function closeCellModal() {
		if (!cellModal) return
		selectedCell = { row: cellModal.row, col: cellModal.col }
		cellModal = null
		focusGridShell()
	}

	function onInputKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' && e.shiftKey) {
			e.preventDefault()
			e.stopPropagation()
			const row = editing?.row
			const col = editing?.col
			const current = draft
			if (row == null || !col) return
			openCellModal(row, col, current)
			return
		}
		if (e.key === 'Enter') {
			e.preventDefault()
			e.stopPropagation()
			commitEdit()
			// Excel-like: Enter moves down after commit
			moveSelection(1, 0)
		} else if (e.key === 'Escape') {
			e.preventDefault()
			e.stopPropagation()
			cancelEdit()
		} else if (e.key === 'Tab') {
			e.preventDefault()
			e.stopPropagation()
			commitEdit()
			moveSelection(0, e.shiftKey ? -1 : 1)
		}
	}

	function copySelectedCell(e?: ClipboardEvent) {
		if (!selectedCell || editing) return
		const v = rows[selectedCell.row]?.[selectedCell.col]
		// Clipboard: empty string for NULL (paste-friendly)
		const text = v === null || v === undefined ? '' : String(v)
		if (e?.clipboardData) {
			e.preventDefault()
			e.clipboardData.setData('text/plain', text)
		} else if (navigator.clipboard?.writeText) {
			void navigator.clipboard.writeText(text)
		}
		statusMsg = 'Cell copied'
		setTimeout(() => {
			if (statusMsg === 'Cell copied') statusMsg = null
		}, 1200)
	}

	function pasteIntoGrid(text: string) {
		const cols = colNames()
		if (cols.length === 0) return

		const lines = text.replace(/\r\n/g, '\n').replace(/\r/g, '\n').split('\n')
		if (lines.length > 1 && lines[lines.length - 1] === '') lines.pop()
		if (lines.length === 0) return

		const parsed = lines.map((line) => line.split('\t'))
		const isScalar = parsed.length === 1 && parsed[0]!.length === 1
		const targets = actionCells().filter((c) => !deletedIndices.has(c.row))

		// One clipboard value → fill every selected cell (or the focused cell).
		if (isScalar && targets.length > 0) {
			const value = parsed[0]![0] ?? ''
			for (const { row, col } of targets) {
				applyCellValue(row, col, value)
			}
			const n = targets.length
			flashStatus(n === 1 ? 'Pasted 1 cell' : `Pasted into ${n} cells`)
			return
		}

		const start =
			(targets.length
				? targets.reduce((best, c) => {
						if (c.row < best.row) return c
						if (
							c.row === best.row &&
							cols.indexOf(c.col) < cols.indexOf(best.col)
						) {
							return c
						}
						return best
					})
				: null) ??
			(selectedCell ? { row: selectedCell.row, col: selectedCell.col } : null)
		if (!start) return
		const startColIdx = cols.indexOf(start.col)
		if (startColIdx < 0) return

		let filterIdx = pageEntries.findIndex((e) => e.index === start.row)
		if (filterIdx < 0) filterIdx = 0

		let changed = 0
		for (let r = 0; r < parsed.length; r++) {
			const cells = parsed[r]!
			const entry = pageEntries[filterIdx + r]
			if (!entry) break
			const absRow = entry.index
			if (deletedIndices.has(absRow)) continue
			for (let c = 0; c < cells.length; c++) {
				const colName = cols[startColIdx + c]
				if (!colName) break
				applyCellValue(absRow, colName, cells[c] ?? '')
				changed += 1
			}
		}

		flashStatus(changed === 1 ? 'Pasted 1 cell' : `Pasted ${changed} cells`)
	}

	function onGridKeydown(e: KeyboardEvent) {
		if (cellModal) return
		const mod = e.metaKey || e.ctrlKey

		// ⌘/Ctrl+S — save (works even while a cell is being edited)
		if (mod && !e.altKey && !e.shiftKey && (e.key === 's' || e.key === 'S')) {
			e.preventDefault()
			e.stopPropagation()
			void saveChanges()
			return
		}
		// ⌘/Ctrl+N — new empty row
		if (mod && !e.altKey && !e.shiftKey && (e.key === 'n' || e.key === 'N')) {
			e.preventDefault()
			e.stopPropagation()
			addNewRow()
			return
		}
		// ⌘/Ctrl+D — clone focused / selected row
		if (mod && !e.altKey && !e.shiftKey && (e.key === 'd' || e.key === 'D')) {
			e.preventDefault()
			e.stopPropagation()
			cloneFocusedRow()
			return
		}

		// Ignore when focus is in filter bar / other controls outside shell table
		const t = e.target as HTMLElement | null
		if (
			t &&
			(t.tagName === 'INPUT' ||
				t.tagName === 'TEXTAREA' ||
				t.tagName === 'SELECT' ||
				t.isContentEditable)
		) {
			// Editing cell input has its own handler
			if (editing && (t === inputEl || inputEl?.contains(t))) return
			// Filter inputs etc.
			if (!editing) return
		}

		if (editing) return

		if (mod && (e.key === 'c' || e.key === 'C')) {
			e.preventDefault()
			if (selKind === 'cells' && selectedCells.size > 1) {
				void copySelection('copy')
			} else if (selKind === 'rows' && selectedRows.size > 1) {
				void copyRowAs(selectedRowList(), 'copy')
			} else {
				copySelectedCell()
			}
			return
		}
		// Ctrl/Cmd+V: let the `paste` event handle (has clipboardData).

		if (e.key === 'Enter' && selectedCell) {
			e.preventDefault()
			if (e.shiftKey) {
				openCellModal(selectedCell.row, selectedCell.col)
			} else {
				startEdit(selectedCell.row, selectedCell.col)
			}
			return
		}

		if (e.key === 'F2' && selectedCell) {
			e.preventDefault()
			startEdit(selectedCell.row, selectedCell.col)
			return
		}

		// ⌘/Ctrl+Delete — mark focused row for delete.
		if (
			mod &&
			!e.altKey &&
			!e.shiftKey &&
			(e.key === 'Delete' || e.key === 'Backspace')
		) {
			const targets = actionRows()
			if (targets.length > 0) {
				e.preventDefault()
				const pending = targets.filter((i) => deletedIndices.has(i))
				const live = targets.filter((i) => !deletedIndices.has(i))
				if (live.length) deleteRows(live)
				else if (pending.length) undeleteRows(pending)
				return
			}
		}

		// Delete / Backspace (no mod) — set selected cell(s) to NULL.
		if (
			(e.key === 'Delete' || e.key === 'Backspace') &&
			!mod &&
			!e.altKey &&
			actionCells().length > 0
		) {
			e.preventDefault()
			setCellsNull(actionCells())
			return
		}

		if (e.key === 'Escape') {
			e.preventDefault()
			if (selKind === 'cells' && selectedCells.size > 1 && selectedCell) {
				setExclusiveCell(selectedCell.row, selectedCell.col)
				return
			}
			if (selKind === 'rows' && selectedRows.size > 1 && highlightRow != null) {
				setExclusiveRow(highlightRow)
				return
			}
			if (selectedCell) {
				selectedCell = null
				clearCellSelection()
				selKind = 'none'
				return
			}
			return
		}

		if (mod && !e.altKey && (e.key === 'a' || e.key === 'A')) {
			e.preventDefault()
			selKind = 'rows'
			clearCellSelection()
			selectedCell = null
			selectedRows = new Set(pageEntries.map((e) => e.index))
			if (selectedRows.size > 0 && selectionAnchor == null) {
				selectionAnchor = pageEntries[0]!.index
			}
			flashStatus(
				selectedRows.size === 1
					? '1 row selected'
					: `${selectedRows.size} rows selected`,
			)
			return
		}

		if (e.key === 'ArrowUp') {
			e.preventDefault()
			moveSelection(-1, 0, e.shiftKey)
			return
		}
		if (e.key === 'ArrowDown') {
			e.preventDefault()
			moveSelection(1, 0, e.shiftKey)
			return
		}
		if (e.key === 'ArrowLeft') {
			e.preventDefault()
			moveSelection(0, -1, e.shiftKey)
			return
		}
		if (e.key === 'ArrowRight') {
			e.preventDefault()
			moveSelection(0, 1, e.shiftKey)
			return
		}
		if (e.key === 'Tab') {
			e.preventDefault()
			moveSelection(0, e.shiftKey ? -1 : 1)
			return
		}

		// Type-to-edit: printable character replaces cell content
		if (selectedCell && !mod && !e.altKey && e.key.length === 1 && !e.metaKey) {
			const def = columnDef(selectedCell.col)
			const prev = rows[selectedCell.row]?.[selectedCell.col]
			// Selects: open editor so user picks from list
			if (def && usesSelectEditor(def, prev)) {
				e.preventDefault()
				startEdit(selectedCell.row, selectedCell.col)
				return
			}
			e.preventDefault()
			startEdit(selectedCell.row, selectedCell.col, undefined, e.key)
		}
	}

	function onGridCopy(e: ClipboardEvent) {
		if (editing) return
		if (selKind === 'cells' && selectedCells.size > 1) {
			e.preventDefault()
			void copySelection('copy')
			return
		}
		if (selKind === 'rows' && selectedRows.size > 1) {
			e.preventDefault()
			void copyRowAs(selectedRowList(), 'copy')
			return
		}
		copySelectedCell(e)
	}

	function onGridPaste(e: ClipboardEvent) {
		if (editing) return
		const text = e.clipboardData?.getData('text/plain')
		if (text == null) return
		e.preventDefault()
		pasteIntoGrid(text)
	}

	// Clear cell focus when table reloads
	$effect(() => {
		void table.id
		selectedCell = null
		selectedRows = new Set()
		selectionAnchor = null
		selectedCells = new Set()
		cellAnchor = null
		selKind = 'none'
	})

	async function goToPage(nextPage: number) {
		if (editing) commitEdit()
		if (dirtyCount > 0) {
			statusMsg = 'Save or reset changes before changing page.'
			return
		}
		const p = Math.max(0, Math.min(totalPages - 1, nextPage))
		if (p === safePage) return
		page = p
		if (!onreload) return
		refreshing = true
		statusMsg = null
		try {
			await onreload(reloadOpts(p * PAGE_SIZE))
			loadFromTable(true)
			appliedSyncTick = rowSyncTick
		} catch (err) {
			statusMsg = err instanceof Error ? err.message : String(err)
		} finally {
			refreshing = false
		}
	}

	function goPrev() {
		void goToPage(safePage - 1)
	}

	function goNext() {
		void goToPage(safePage + 1)
	}

	async function refresh() {
		if (refreshing) return
		if (editing) cancelEdit()
		refreshing = true
		statusMsg = null
		try {
			if (onreload) {
				await onreload(reloadOpts(page * PAGE_SIZE))
				loadFromTable(true)
				appliedSyncTick = rowSyncTick
			} else {
				const cloned = seed.map((r) => ({ ...r }))
				rows = cloned.map((r) => ({ ...r }))
				originals = cloned.map((r) => ({ ...r }))
				table.rows.splice(
					0,
					table.rows.length,
					...cloned.map((r) => ({ ...r })),
				)
				dirtyKeys = new Set()
				newRowIndices = new Set()
				deletedIndices = new Set()
			}
			page = 0
			statusMsg = 'Refreshed'
			onbaselinechange?.()
			setTimeout(() => {
				if (statusMsg === 'Refreshed') statusMsg = null
			}, 2000)
		} catch (err) {
			statusMsg = err instanceof Error ? err.message : String(err)
		} finally {
			refreshing = false
		}
	}

	async function saveChanges() {
		if (saving) return
		if (editing) commitEdit()
		if (dirtyCount === 0) return
		saving = true
		statusMsg = null
		try {
			const pendingDbDeletes = [...deletedIndices].filter(
				(i) => !newRowIndices.has(i),
			).length
			const persist = (sid: string) =>
				persistTableEdits({
					sessionId: sid,
					engine,
					table,
					rows,
					originals,
					newRowIndices,
					deletedIndices,
					dirtyKeys,
					profileReadOnly,
				})
			let sid = sessionId
			if (!sid && onreconnect) sid = await onreconnect()
			let result
			try {
				result = await persist(sid ?? '')
			} catch (err) {
				if (!onreconnect || !isSessionMissingError(err)) throw err
				const next = await onreconnect()
				if (!next) throw err
				result = await persist(next)
			}
			if (pendingDbDeletes > 0 && (result.deleted ?? 0) < pendingDbDeletes) {
				throw new Error(
					'Delete did not apply. Restart MystiqueDB (`npm run tauri dev`) so the engine can run DELETE.',
				)
			}
			if (onreload) {
				await onreload(reloadOpts(page * PAGE_SIZE))
				loadFromTable(true)
				appliedSyncTick = rowSyncTick
			} else {
				const snap = rows
					.map((r, i) => ({ r, i }))
					.filter(({ i }) => !deletedIndices.has(i))
					.map(({ r }) => ({ ...r }))
				originals = snap.map((r) => ({ ...r }))
				seed = snap.map((r) => ({ ...r }))
				rows = snap.map((r) => ({ ...r }))
				table.rows.splice(0, table.rows.length, ...snap.map((r) => ({ ...r })))
				dirtyKeys = new Set()
				newRowIndices = new Set()
				deletedIndices = new Set()
			}
			const bits = []
			if (result.updated) bits.push(`${result.updated} updated`)
			if (result.inserted) bits.push(`${result.inserted} inserted`)
			if (result.deleted) bits.push(`${result.deleted} deleted`)
			statusMsg = bits.length > 0 ? `Saved (${bits.join(', ')})` : 'Saved'
			onbaselinechange?.()
			setTimeout(() => {
				if (statusMsg?.startsWith('Saved')) statusMsg = null
			}, 2500)
		} catch (err) {
			statusMsg = err instanceof Error ? err.message : String(err)
		} finally {
			saving = false
		}
	}

	// Global shortcuts + File→Save while this table is the active tab.
	onMount(() => {
		const onKey = (e: KeyboardEvent) => {
			if (!(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey) return
			const k = e.key.toLowerCase()
			if (k === 's') {
				e.preventDefault()
				e.stopPropagation()
				void saveChanges()
				return
			}
			if (k === 'n') {
				e.preventDefault()
				e.stopPropagation()
				addNewRow()
				return
			}
			if (k === 'd') {
				e.preventDefault()
				e.stopPropagation()
				cloneFocusedRow()
				return
			}
			if (k === 'r') {
				// Block browser reload; refresh table data instead.
				e.preventDefault()
				e.stopPropagation()
				void refresh()
			}
		}
		const onMenuSave = () => {
			void saveChanges()
		}
		window.addEventListener('keydown', onKey, true)
		window.addEventListener('mystique:save-table', onMenuSave)
		return () => {
			window.removeEventListener('keydown', onKey, true)
			window.removeEventListener('mystique:save-table', onMenuSave)
		}
	})

	function resetChanges() {
		if (editing) cancelEdit()
		if (dirtyCount === 0) return

		// Drop locally added rows, restore the rest from baseline
		if (newRowIndices.size > 0) {
			const keep = rows
				.map((r, i) => ({ r, i }))
				.filter(({ i }) => !newRowIndices.has(i))
			rows = keep.map(({ r }) => ({ ...r }))
			originals = keep.map(({ i }) => ({ ...(originals[i] ?? {}) }))
			seed = keep.map(({ i }) => ({ ...(seed[i] ?? {}) }))
			newRowIndices = new Set()
		}
		deletedIndices = new Set()

		const restored = originals.map((r) => ({ ...r }))
		rows = restored.map((r) => ({ ...r }))
		table.rows.splice(0, table.rows.length, ...restored.map((r) => ({ ...r })))
		dirtyKeys = new Set()
		selectedCell = null
		statusMsg = 'Changes discarded'
		onbaselinechange?.()
		setTimeout(() => {
			if (statusMsg === 'Changes discarded') statusMsg = null
		}, 2000)
	}
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col">
	<GridFilterBar
		{filterMode}
		{filterRules}
		bind:filterCode
		{columnOptions}
		onSetMode={setFilterMode}
		onUpdateFirst={updateFirstRule}
		onUpdateRule={updateRule}
		onRemoveRule={removeRule}
		onAddRule={addRule}
		onApply={applyFilters}
		onClear={clearFilters}
		codePlaceholder={codeFilterPlaceholder}
	/>

	<!-- tabindex so arrow/copy/paste work when grid is focused -->
	<div
		bind:this={gridShellEl}
		class="flex min-h-0 min-w-0 flex-1 flex-col outline-none"
		tabindex="0"
		role="grid"
		aria-label="Table data"
		onkeydown={onGridKeydown}
		oncopy={onGridCopy}
		onpaste={onGridPaste}
	>
		<GridTable
			{table}
			{pageEntries}
			selectedRowIndex={selKind === 'rows'
				? (highlightRow ?? selectedRowIndex)
				: null}
			isRowSelected={(row) => selKind === 'rows' && selectedRows.has(row)}
			isCellSelected={(row, col) =>
				selKind === 'cells' && selectedCells.has(selCellKey(row, col))}
			{selectedCell}
			{originals}
			{editing}
			bind:draft
			bind:inputEl
			{filtersActive}
			{hasActiveFilters}
			{sortColumn}
			{sortDir}
			{isDirty}
			isNewRow={(row) => newRowIndices.has(row)}
			isDeletedRow={(row) => deletedIndices.has(row)}
			onSelectCell={selectCell}
			onSelectRow={selectRowOnly}
			onStartEdit={startEdit}
			{onInputKeydown}
			onCommitEdit={commitEdit}
			onRowMenu={openRowMenu}
			onCellMenu={openCellMenu}
			onOpenReference={onopenreference}
			onSort={cycleSort}
		/>
	</div>

	{#if cellModal}
		<CellValueModal
			column={cellModal.col}
			columnType={cellModal.type}
			rowIndex={cellModal.row}
			initialText={cellModal.text}
			onapply={applyCellModal}
			onclose={closeCellModal}
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
			onpick={(id) => void onCellMenuPick(id)}
			onclose={() => (cellMenu = null)}
		/>
	{/if}

	<GridStatusBar
		{filteredCount}
		{pageStart}
		{pageEnd}
		{filtersActive}
		{hasActiveFilters}
		tableRowCount={Math.max(table.rowCount, rows.length)}
		{dirtyCount}
		{statusMsg}
		{safePage}
		{totalPages}
		{refreshing}
		{saving}
		onPrev={goPrev}
		onNext={goNext}
		onRefresh={() => void refresh()}
		onAddRow={addNewRow}
		onReset={resetChanges}
		onSave={() => void saveChanges()}
	/>
</div>
