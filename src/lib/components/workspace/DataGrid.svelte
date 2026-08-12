<script lang="ts">
	import type { TablePreviewFilter } from '$lib/api/engines'
	import type { ColumnReference } from '$lib/types/workspace'
	import { persistTableEdits } from '$lib/api/tablePersist'
	import { ContextMenu, type ContextMenuItem } from '$lib/components/ui'
	import { readInspectOnRowClick } from '$lib/utils/appPrefs'
	import { isSessionMissingError } from './shell/sessionUtils'
	import type { TableMeta } from '$lib/types/workspace'
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

	interface Props {
		table: TableMeta
		selectedRowIndex: number | null
		onselectrow: (index: number) => void
		rowSyncTick?: number
		onbaselinechange?: () => void
		/** Notify parent when unsaved edit count changes (for tab dirty indicator). */
		ondirtychange?: (dirty: boolean) => void
		engine?: string
		sessionId?: string | null
		profileReadOnly?: boolean
		/** Reload live rows from the engine (Refresh, pager, save). */
		onreload?: (opts?: {
			offset?: number
			filters?: TablePreviewFilter[]
		}) => Promise<void>
		/** Re-open a dead engine session; returns the new session id. */
		onreconnect?: () => Promise<string | null>
		/**
		 * When opening a table via FK jump, seed the filter bar so pager/refresh
		 * keep the same predicate the plugin already applied.
		 */
		seedFilters?: TablePreviewFilter[] | null
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
		engine = '',
		sessionId = null,
		profileReadOnly = false,
		onreload,
		onreconnect,
		seedFilters = null,
		onopenreference,
	}: Props = $props()

	let seed = $state<Record<string, unknown>[]>([])
	let rows = $state<Record<string, unknown>[]>([])
	let originals = $state<Record<string, unknown>[]>([])
	let dirtyKeys = $state<Set<string>>(new Set())
	/** Absolute indices of rows added locally (not yet saved). */
	let newRowIndices = $state<Set<number>>(new Set())
	/** Absolute indices of persisted rows marked for delete (not yet saved). */
	let deletedIndices = $state<Set<number>>(new Set())
	let rowMenu = $state<{ row: number; x: number; y: number } | null>(null)
	let editing = $state<CellRef | null>(null)
	/** Focused cell for keyboard nav / copy-paste (absolute row index). */
	let selectedCell = $state<CellRef | null>(null)
	/** Local row highlight — inspector only opens via Show details. */
	let highlightRow = $state<number | null>(null)
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

	const hasActiveFilters = $derived(activeRules().length > 0)

	function previewFilters(): TablePreviewFilter[] {
		if (!filtersActive) return []
		return activeRules().map((r) => ({
			column: r.column,
			op: r.op,
			value: r.value,
		}))
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
			const parsed = parseCodeFilters(filterCode)
			if (parsed && parsed.length > 0) {
				filterRules = parsed
			} else if (filterRules.length === 0) {
				filterRules = [makeRule()]
			}
		}
		filterMode = mode
	}

	async function reloadFromEngine(offset: number) {
		if (!onreload) return
		refreshing = true
		statusMsg = null
		try {
			await onreload({ offset, filters: previewFilters() })
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
		if (filterMode === 'code' && parseCodeFilters(filterCode) == null) {
			flashStatus('Could not parse filter expression')
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
		// First preview often arrives after mount with an empty baseline.
		// If the user has no local edits, take incoming rows as the new baseline
		// so non-null cells are not marked dirty.
		const hasLocalEdits =
			dirtyKeys.size > 0 || newRowIndices.size > 0 || deletedIndices.size > 0
		if (resetBaseline || !hasLocalEdits || originals.length === 0) {
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
		const shiftSet = (set: Set<number>) =>
			new Set([...set].map((i) => i + 1))
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
		const row = selectedCell?.row ?? highlightRow ?? selectedRowIndex
		if (row == null) {
			flashStatus('Select a row to clone')
			return
		}
		cloneRow(row)
	}

	function flashStatus(msg: string, ms = 1600) {
		statusMsg = msg
		setTimeout(() => {
			if (statusMsg === msg) statusMsg = null
		}, ms)
	}

	function rowExportPayload(index: number) {
		const row = rows[index]
		const columns = table.columns.map((c) => ({ name: c.name }))
		const objects = row ? [row] : []
		const cells = objectsToRows(columns, objects)
		return { columns, cells, row }
	}

	async function copyRowAs(
		index: number,
		kind: 'copy' | 'csv' | 'markdown' | 'json',
	) {
		const { columns, cells, row } = rowExportPayload(index)
		if (!row) return
		let text = ''
		let okMsg = 'Copied'
		switch (kind) {
			case 'copy':
				text = buildTsv(columns, cells)
				okMsg = 'Row copied'
				break
			case 'csv':
				text = buildCsv(columns, cells)
				okMsg = 'Copied as CSV'
				break
			case 'markdown':
				text = buildMarkdown(columns, cells)
				okMsg = 'Copied as Markdown'
				break
			case 'json':
				text = buildJson(columns, cells, true)
				okMsg = 'Copied as JSON'
				break
		}
		const res = await copyTextToClipboard(text)
		flashStatus(res.ok ? okMsg : res.cancelled ? 'Copy cancelled' : res.error)
	}

	function cloneRow(index: number) {
		if (profileReadOnly) {
			flashStatus('Connection is read-only')
			return
		}
		if (editing) commitEdit()
		const src = rows[index]
		if (!src) return
		// Snapshot before prepend shifts indices.
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
		insertLocalRowAtTop(row)
		flashStatus('Row cloned — save to insert')
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
	}

	function deleteRow(index: number) {
		if (profileReadOnly) {
			flashStatus('Connection is read-only')
			return
		}
		if (editing) commitEdit()
		if (!rows[index]) return
		if (deletedIndices.has(index)) return

		// Unsaved insert/clone: remove locally. Staging a DELETE has no PK and
		// blocks Save (dirtyCount treats new+deleted as a no-op).
		if (newRowIndices.has(index)) {
			removeLocalRow(index)
			flashStatus('Unsaved row removed')
			return
		}

		const hasPk = table.columns.some(
			(c) =>
				c.pk || c.name === 'id' || c.name === '_id' || c.name === 'rowid',
		)
		if (!hasPk) {
			flashStatus('Cannot delete: table has no primary key')
			return
		}

		deletedIndices = new Set([...deletedIndices, index])
		if (editing?.row === index) {
			editing = null
			draft = ''
		}
		if (selectedCell?.row === index) selectedCell = null
		flashStatus('Marked for delete — save to apply')
	}

	function undeleteRow(index: number) {
		if (!deletedIndices.has(index)) return
		deletedIndices = new Set([...deletedIndices].filter((i) => i !== index))
		flashStatus('Delete undone')
	}

	const rowMenuItems = $derived.by((): ContextMenuItem[] => {
		const ro = profileReadOnly
		const pending = rowMenu != null && deletedIndices.has(rowMenu.row)
		return [
			{ id: 'copy', label: 'Copy' },
			{ id: 'csv', label: 'Copy as CSV' },
			{ id: 'markdown', label: 'Copy as Markdown' },
			{ id: 'json', label: 'Copy as JSON' },
			{
				id: 'clone',
				label: 'Clone row',
				shortcut: '⌘/Ctrl+D',
				disabled: ro || pending,
				separatorBefore: true,
			},
			pending
				? { id: 'undelete', label: 'Undo delete', disabled: ro }
				: {
						id: 'delete',
						label: 'Delete row',
						danger: true,
						disabled: ro,
					},
			{
				id: 'details',
				label: 'Show details',
				separatorBefore: true,
			},
		]
	})

	function openRowMenu(row: number, e: MouseEvent) {
		rowMenu = { row, x: e.clientX, y: e.clientY }
	}

	function onRowMenuPick(id: string) {
		const row = rowMenu?.row
		rowMenu = null
		if (row == null) return
		if (id === 'copy' || id === 'csv' || id === 'markdown' || id === 'json') {
			void copyRowAs(row, id)
			return
		}
		if (id === 'clone') {
			cloneRow(row)
			return
		}
		if (id === 'delete') deleteRow(row)
		if (id === 'undelete') undeleteRow(row)
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
		// Seed filters from FK jump (or clear when opening a normal table).
		const seed = seedFilters
		if (seed && seed.length > 0) {
			filterRules = seed.map((f) => ({
				id: `f-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
				column: f.column,
				op: (f.op as FilterOp) || 'eq',
				value: f.value ?? '',
			}))
			filterCode = rulesToCode(filterRules)
			filtersActive = true
		} else {
			filtersActive = false
			filterRules = [makeRule()]
			filterCode = ''
		}
		filterMode = 'ui'
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
		return readInspectOnRowClick() || selectedRowIndex != null
	}

	function selectCell(row: number, col: string) {
		if (editing) {
			if (editing.row === row && editing.col === col) return
			commitEdit()
		}
		selectedCell = { row, col }
		highlightRow = row
		if (shouldOpenInspector()) onselectrow(row)
		focusGridShell()
		scrollCellIntoView(row, col)
	}

	function selectRowOnly(row: number) {
		if (editing) commitEdit()
		selectedCell = null
		highlightRow = row
		if (shouldOpenInspector()) onselectrow(row)
		focusGridShell()
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
	}

	/** Current page is already fetched from the server; just scroll. */
	function ensureRowVisible(absoluteRow: number) {
		const col = selectedCell?.col ?? colNames()[0]
		if (col) scrollCellIntoView(absoluteRow, col)
	}

	function moveSelection(dRow: number, dCol: number) {
		const cols = colNames()
		if (cols.length === 0 || pageEntries.length === 0) return

		let row = selectedCell?.row ?? pageEntries[0]!.index
		let colIdx = selectedCell ? Math.max(0, cols.indexOf(selectedCell.col)) : 0

		// Map absolute row → position in the current engine page
		let filterIdx = pageEntries.findIndex((e) => e.index === row)
		if (filterIdx < 0) filterIdx = 0

		filterIdx = Math.max(
			0,
			Math.min(pageEntries.length - 1, filterIdx + dRow),
		)
		colIdx = Math.max(0, Math.min(cols.length - 1, colIdx + dCol))

		const nextAbs = pageEntries[filterIdx]!.index
		const nextCol = cols[colIdx]!
		ensureRowVisible(nextAbs)
		selectedCell = { row: nextAbs, col: nextCol }
		highlightRow = nextAbs
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
		if (!selectedCell) return
		const cols = colNames()
		if (cols.length === 0) return

		const startColIdx = cols.indexOf(selectedCell.col)
		if (startColIdx < 0) return

		// Normalize line endings; support TSV multi-cell paste from sheets
		const lines = text.replace(/\r\n/g, '\n').replace(/\r/g, '\n').split('\n')
		// Drop trailing empty line from Excel
		if (lines.length > 1 && lines[lines.length - 1] === '') lines.pop()
		if (lines.length === 0) return

		let filterIdx = pageEntries.findIndex(
			(e) => e.index === selectedCell!.row,
		)
		if (filterIdx < 0) filterIdx = 0

		let changed = 0
		for (let r = 0; r < lines.length; r++) {
			const cells = lines[r]!.split('\t')
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

		statusMsg = changed === 1 ? 'Pasted 1 cell' : `Pasted ${changed} cells`
		setTimeout(() => {
			if (statusMsg?.startsWith('Pasted')) statusMsg = null
		}, 1500)
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
			copySelectedCell()
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

		if (e.key === 'Escape') {
			e.preventDefault()
			selectedCell = null
			return
		}

		if (e.key === 'ArrowUp') {
			e.preventDefault()
			moveSelection(-1, 0)
			return
		}
		if (e.key === 'ArrowDown') {
			e.preventDefault()
			moveSelection(1, 0)
			return
		}
		if (e.key === 'ArrowLeft') {
			e.preventDefault()
			moveSelection(0, -1)
			return
		}
		if (e.key === 'ArrowRight') {
			e.preventDefault()
			moveSelection(0, 1)
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
			await onreload({ offset: p * PAGE_SIZE, filters: previewFilters() })
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
				await onreload({
					offset: page * PAGE_SIZE,
					filters: previewFilters(),
				})
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
				await onreload({
					offset: page * PAGE_SIZE,
					filters: previewFilters(),
				})
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
			selectedRowIndex={highlightRow ?? selectedRowIndex}
			{selectedCell}
			{originals}
			{editing}
			bind:draft
			bind:inputEl
			{filtersActive}
			{hasActiveFilters}
			{isDirty}
			isNewRow={(row) => newRowIndices.has(row)}
			isDeletedRow={(row) => deletedIndices.has(row)}
			onSelectCell={selectCell}
			onSelectRow={selectRowOnly}
			onStartEdit={startEdit}
			{onInputKeydown}
			onCommitEdit={commitEdit}
			onRowMenu={openRowMenu}
			onOpenReference={onopenreference}
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
