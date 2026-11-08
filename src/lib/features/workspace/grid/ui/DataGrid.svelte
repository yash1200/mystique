<script lang="ts">
	import type {
		ColumnReference,
		TableMeta,
		TablePreviewFilter,
		TablePreviewSortDir,
	} from '#core/contracts'
	import { commands, type CommandContext } from '#core/commands'
	import { ContextMenu, type ContextMenuItem } from '#core/ui'
	import { fetchEngineLargeValue, unlockEngineWrites } from '#core/ipc'
	import { copyTextToClipboard, latestRun } from '#core/util'
	import { showStatus, showToast } from '#features/toast'
	import {
		adoptServerRows,
		askWriteGuard,
		getEditBuffer,
		isWriteCancelled,
		registerTabSaver,
		rememberEditBuffer,
		shouldRestoreEditBuffer,
		tabEditsAreDirty,
		type SaveOutcome,
	} from '#features/workspace/session'
	import { appPrefs, cmd } from '#lib/app'
	import { onMount, tick, untrack } from 'svelte'
	import {
		distinctColumnValues,
		loadColumnLayout,
		loadLatestSaveId,
		persistTableEdits,
		rollbackTableEdits,
		saveColumnLayout,
	} from '../api'
	import {
		columnCompletions,
		distinctCacheKey,
		readDistinctCache,
		DISTINCT_TTL_MS,
		type DistinctCacheEntry,
	} from '../column-complete'
	import { cloneRowValues, type FetchChunk } from '../clone-row'
	import {
		columnRange,
		columnsFrom,
		inVisibleOrder,
		stepColumn,
		visibleColumns,
	} from '../visible-columns'
	import {
		EMPTY_COLUMN_LAYOUT,
		moveColumn,
		toggleFrozen,
		toggleHidden,
		withColumnWidth,
		type GridColumnLayout,
	} from '../column-layout'
	import {
		buildInList,
		buildInsertSql,
		buildUpdateSql,
		buildWhereSql,
		identStyleFor,
		type CopyColumn,
	} from '../copy-as'
	import {
		cloneSnapshot,
		pushUndo,
		redoEdit,
		undoEdit,
		type GridEditSnapshot,
	} from '../edit-history'
	import { gridShortcutBlocked } from '../grid-keys'
	import { applyCellBatch, type CellEdit } from '../paste-batch'
	import { formatSelectionStats, selectionStats } from '../selection-stats'
	import { valueToEditorText } from '../cell-view'
	import {
		cellsIncludeLarge,
		isLargeCell,
		largeCellText,
		PARTIAL_COPY_MESSAGE,
		type LargeCellLocator,
	} from '../large-cell'
	import {
		selectDraftToValue,
		usesSelectEditor,
		valueToSelectDraft,
	} from '../column-editor'
	import {
		buildCsv,
		buildJson,
		buildMarkdown,
		buildTsv,
		objectsToRows,
	} from '../export'
	import {
		cellText,
		looksLikeJsonFilter,
		newRule,
		opNeedsValue,
		parseCodeFilters,
		parseDraft,
		rulesToCode,
		valuesEqual,
	} from '../filters'
	import type { FilterMode, FilterOp, FilterRule } from '../types'
	import CellValueModal from './CellValueModal.svelte'
	import GridFilterBar from './GridFilterBar.svelte'
	import GridStatusBar from './GridStatusBar.svelte'
	import GridTable, { type CellRef } from './GridTable.svelte'
	import GridSqlPanel from './GridSqlPanel.svelte'
	import {
		reviewPrompt,
		reviewsEverySave,
		savedMessage,
		undoSave,
	} from '../save-flow'

	interface Props {
		table: TableMeta
		selectedRowIndex: number | null
		onselectrow: (index: number) => void
		rowSyncTick?: number
		/** Server replaced the preview page. Adopt as the new saved baseline. */
		tablePreviewTick?: number
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
		/** Open this table's structure in a new tab. */
		onopenstructure?: () => void
		/** Tab id for the edit buffer. Remounts rehydrate this instead of the catalog. */
		tabId?: string | null
		connectionId?: string | null
		sqlDialect?: string
		supportsDistinct?: boolean
		/** Profile environment is production (save review dialog copy). */
		production?: boolean
		connectionName?: string
	}

	let {
		table,
		selectedRowIndex,
		onselectrow,
		rowSyncTick = 0,
		tablePreviewTick = 0,
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
		onopenstructure,
		tabId = null,
		connectionId = null,
		sqlDialect = '',
		supportsDistinct = true,
		production = false,
		connectionName = '',
	}: Props = $props()

	let seed = $state<Record<string, unknown>[]>([])
	let rows = $state.raw<Record<string, unknown>[]>([])
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
	let gridTable = $state<ReturnType<typeof GridTable> | null>(null)
	let cellModal = $state<{
		row: number
		col: string
		type: string
		text: string
		locator: LargeCellLocator | null
		unfetchable: boolean
	} | null>(null)

	const PAGE_SIZE = 100
	let page = $state(0)
	let refreshing = $state(false)
	/** Second ⌘R confirms discarding a dirty buffer. */
	let refreshArmed = $state(false)
	let saving = $state(false)
	let columnLayout = $state<GridColumnLayout>({ ...EMPTY_COLUMN_LAYOUT })
	/** Visible order (frozen, then the rest; no hidden). Every grid operation maps through this. */
	const visible = $derived(visibleColumns(table.columns, columnLayout))
	/** Backend id of the save the Rollback button undoes. */
	let lastSaveId = $state<string | null>(null)
	let undoStack = $state<GridEditSnapshot[]>([])
	let redoStack = $state<GridEditSnapshot[]>([])
	/** Live "Show SQL" panel contents. */
	let sqlPanelStatements = $state<string[]>([])
	let sqlPanelError = $state<string | null>(null)
	let codeCompletions = $state<string[]>([])
	let completionStart = $state(0)
	let distinctValues = $state<string[]>([])
	let columnMenu = $state<{ column: string; x: number; y: number } | null>(null)
	const distinctCache = new Map<string, DistinctCacheEntry>()
	let restoringHistory = false
	let loadedTableId = $state<string | null>(null)
	let appliedSyncTick = $state(0)
	let appliedPreviewTick = $state(0)
	let appliedSeedSig = $state<string | null>(null)

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

	const selectionLabel = $derived.by(() => {
		const values: unknown[] = []
		if (selKind === 'cells') {
			for (const key of selectedCells) {
				const sep = key.indexOf('::')
				if (sep < 0) continue
				const row = Number(key.slice(0, sep))
				const col = key.slice(sep + 2)
				if (!Number.isFinite(row)) continue
				values.push(rows[row]?.[col])
			}
		} else if (selKind === 'rows') {
			for (const index of selectedRows) {
				const row = rows[index]
				if (!row) continue
				for (const column of table.columns) {
					if (columnLayout.hidden.includes(column.name)) continue
					values.push(row[column.name])
				}
			}
		} else if (selectedCell) {
			values.push(rows[selectedCell.row]?.[selectedCell.col])
		}
		return formatSelectionStats(selectionStats(values))
	})

	$effect(() => {
		ondirtychange?.(dirtyCount > 0)
	})

	// A second refresh confirms a discard. Clearing the edits requires a new confirm.
	$effect(() => {
		if (dirtyCount === 0) refreshArmed = false
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
	const serverTotal = $derived.by(() => {
		const n = Math.max(table.rowCount, rows.length)
		// COUNT is deferred. A full page means there may be another.
		// A short page is the last one, so count the pages already passed.
		if (table.totalKnown === false && rows.length >= PAGE_SIZE) {
			return Math.max(n, (page + 1) * PAGE_SIZE + 1)
		}
		if (table.totalKnown === false) {
			return page * PAGE_SIZE + rows.length
		}
		return n
	})
	const totalPages = $derived(
		Math.max(1, Math.ceil(serverTotal / PAGE_SIZE) || 1),
	)
	const safePage = $derived(Math.min(page, totalPages - 1))
	let lastFetchedAt = $state<number | null>(Date.now())

	$effect(() => {
		void table.id
		void tablePreviewTick
		lastFetchedAt = Date.now()
	})

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

	function markServerPreviewApplied() {
		appliedPreviewTick = tablePreviewTick
		appliedSyncTick = rowSyncTick
	}

	async function reloadFromEngine(offset: number) {
		if (!onreload) return
		refreshing = true
		try {
			await onreload(reloadOpts(offset))
			loadFromTable(true)
			markServerPreviewApplied()
		} catch (err) {
			reportGridError(err)
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

	let distinctTimer: ReturnType<typeof setTimeout> | undefined

	function scheduleDistinct(column: string) {
		if (distinctTimer) clearTimeout(distinctTimer)
		if (!supportsDistinct || !sessionId || !column) {
			distinctValues = []
			return
		}
		const key = distinctCacheKey(sessionId, table.id, column)
		const cached = readDistinctCache(distinctCache, key, Date.now(), DISTINCT_TTL_MS)
		if (cached) {
			distinctValues = cached
			return
		}
		const sid = sessionId
		distinctTimer = setTimeout(() => {
			void distinctColumnValues({ sessionId: sid, table, column })
				.then((values) => {
					distinctCache.set(key, { at: Date.now(), values })
					distinctValues = values
				})
				.catch(() => {
					distinctValues = []
				})
		}, 300)
	}

	function onCodeInput(value: string, caret: number) {
		const found = columnCompletions(
			value,
			caret,
			table.columns.map((column) => column.name),
		)
		completionStart = found.start
		codeCompletions = found.items
	}

	function pickCompletion(item: string) {
		const typed =
			filterCode.slice(completionStart).match(/^[A-Za-z_][A-Za-z0-9_]*/)?.[0] ?? ''
		filterCode =
			filterCode.slice(0, completionStart) +
			item +
			filterCode.slice(completionStart + typed.length)
		codeCompletions = []
	}

	function updateFirstRule(patch: Partial<FilterRule>) {
		if (filterRules.length === 0) {
			filterRules = [{ ...makeRule(), ...patch }]
		} else {
			updateRule(filterRules[0].id, patch)
		}
		if (patch.column) scheduleDistinct(patch.column)
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

	function restoreBuffer(buf: ReturnType<typeof getEditBuffer>) {
		if (!buf) return
		rows = buf.rows.map((r) => ({ ...r }))
		originals = buf.baseline.map((r) => ({ ...r }))
		seed = buf.baseline.map((r) => ({ ...r }))
		dirtyKeys = new Set(buf.dirtyKeys)
		newRowIndices = new Set(buf.newRowIndices)
		deletedIndices = new Set(buf.deletedIndices)
		page = buf.page
		loadedTableId = table.id
		editing = null
		draft = ''
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
			if (tabId) adoptServerRows(tabId, cloned, page)
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
	function insertLocalRowAtTop(row: Record<string, unknown>, track = true) {
		if (track) noteEdit()
		shiftIndicesAfterPrepend()
		const copy = { ...row }
		rows = [copy, ...rows]
		originals = [{ ...copy }, ...originals]
		seed = [{ ...copy }, ...seed]
		newRowIndices = new Set([0, ...newRowIndices])

		const firstCol = table.columns[0]?.name
		if (firstCol) {
			setExclusiveRow(0)
			selectedCell = { row: 0, col: firstCol }
			if (shouldOpenInspector()) onselectrow(0)
			void tick().then(() => {
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
	}

	function cloneFocusedRow() {
		const targets = actionRows()
		if (targets.length === 0) {
			flashStatus('Select a row to duplicate')
			return
		}
		void cloneRows(targets)
	}

	function flashStatus(msg: string) {
		showStatus(msg, 'warning')
	}

	function reportGridError(err: unknown) {
		showToast('danger', err instanceof Error ? err.message : String(err))
	}

	function notifyCopy(
		res: { ok: boolean; cancelled?: boolean; error?: string },
		okMsg: string,
		partial = false,
	) {
		if (res.ok && partial) showToast('warning', okMsg)
		else if (res.ok) showStatus(okMsg)
		else if (!res.cancelled) showToast('danger', res.error || 'Copy failed')
	}

	function assertWritable(): boolean {
		if (!profileReadOnly) return true
		flashStatus('Connection is read-only')
		return false
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
				if (Number.isFinite(row) && visible.names.includes(col)) out.push({ row, col })
			}
			return out
		}
		if (selKind === 'rows' && selectedRows.size > 0) {
			const cols = visible.names
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
		const pending = cells.filter((cell) => {
			if (deletedIndices.has(cell.row)) return false
			const cur = rows[cell.row]?.[cell.col]
			return cur !== null && cur !== undefined
		})
		if (pending.length === 0) return
		noteEdit()
		let n = 0
		for (const { row, col } of pending) {
			applyCellValue(row, col, 'NULL', false)
			n += 1
		}
		if (n === 0) return
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
		const from = cellAnchor ?? selectedCell ?? { row: toRow, col: toCol }
		const names = columnRange(visible, from.col, toCol)
		const r0 = Math.min(from.row, toRow)
		const r1 = Math.max(from.row, toRow)
		const next = new Set<string>()
		for (let r = r0; r <= r1; r++) {
			for (const name of names) next.add(selCellKey(r, name))
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
		const parsed = actionCells()
		if (parsed.length === 0) return null
		const rowIdx = [...new Set(parsed.map((p) => p.row))].sort((a, b) => a - b)
		const names = inVisibleOrder(
			visible,
			parsed.map((p) => p.col),
		)
		const columns = names.map((name) => ({ name }))
		const cells = rowIdx.map((r) =>
			names.map((name) =>
				selectedCells.has(selCellKey(r, name)) ? (rows[r]?.[name] ?? '') : '',
			),
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

	function copyColumns(columns: { name: string }[]): CopyColumn[] {
		return columns.map((column) => {
			const def = table.columns.find((item) => item.name === column.name)
			return {
				name: column.name,
				dataType: def?.type,
				pk: !!def?.pk,
			}
		})
	}

	function sqlText(
		kind: 'insert' | 'update' | 'in' | 'where',
		columns: { name: string }[],
		cells: unknown[][],
	): string {
		const style = identStyleFor(engine, sqlDialect)
		const cols = copyColumns(columns)
		if (kind === 'insert') {
			return buildInsertSql(style, table.name, cols, cells, table.database, table.schema)
		}
		if (kind === 'update') {
			return buildUpdateSql(style, table.name, cols, cells, table.database, table.schema)
		}
		if (kind === 'in') return buildInList(style, cols, cells)
		return buildWhereSql(style, cols, cells)
	}

	async function copyRowAs(
		indices: number[],
		kind: 'copy' | 'csv' | 'markdown' | 'json' | 'insert' | 'update' | 'in' | 'where',
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
			case 'insert':
				text = sqlText('insert', columns, cells)
				okMsg = `Copied ${n} as INSERT`
				break
			case 'update':
				text = sqlText('update', columns, cells)
				okMsg = `Copied ${n} as UPDATE`
				break
			case 'in':
				text = sqlText('in', columns, cells)
				okMsg = `Copied ${n} as IN list`
				break
			case 'where':
				text = sqlText('where', columns, cells)
				okMsg = `Copied ${n} as WHERE`
				break
		}
		const res = await copyTextToClipboard(text)
		const partial = cellsIncludeLarge(cells)
		notifyCopy(
			res,
			partial ? `${okMsg}. ${PARTIAL_COPY_MESSAGE}` : okMsg,
			partial,
		)
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
				const partial = cellsIncludeLarge(payload.cells)
				notifyCopy(
					res,
					partial ? `${okMsg}. ${PARTIAL_COPY_MESSAGE}` : okMsg,
					partial,
				)
				return
			}
		}
		await copyRowAs(fallbackRows ?? actionRows(), kind)
	}

	async function cloneRows(indices: number[]) {
		if (profileReadOnly) {
			flashStatus('Connection is read-only')
			return
		}
		if (editing) commitEdit()
		const sid = sessionId
		const fetchChunk: FetchChunk | null = sid
			? (locator, offset) => fetchEngineLargeValue({ sessionId: sid, locator, offset })
			: null
		const sources = [...new Set(indices)]
			.sort((a, b) => a - b)
			.map((i) => rows[i])
			.filter((r): r is Record<string, unknown> => !!r)
		let snaps: Record<string, unknown>[]
		try {
			snaps = await Promise.all(
				sources.map((row) =>
					cloneRowValues({ row, columns: table.columns, fetchChunk }),
				),
			)
		} catch (err) {
			reportGridError(err)
			return
		}
		if (snaps.length === 0) return
		noteEdit()
		for (let i = snaps.length - 1; i >= 0; i--) {
			const snap = snaps[i]
			if (snap) insertLocalRowAtTop(snap, false)
		}
	}

	function cloneRow(index: number) {
		void cloneRows([index])
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
		noteEdit()
		for (const i of unique) deleteRow(i, true)
	}

	function undeleteRows(indices: number[]) {
		const next = new Set(deletedIndices)
		if (![...indices].some((index) => next.has(index))) return
		noteEdit()
		for (const i of indices) next.delete(i)
		deletedIndices = next
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
			return
		}

		const hasPk = table.columns.some(
			(c) => c.pk || c.name === 'id' || c.name === '_id' || c.name === 'rowid',
		)
		if (!hasPk) {
			if (!quiet) showStatus('Cannot delete: table has no primary key', 'warning')
			return
		}

		deletedIndices = new Set([...deletedIndices, index])
		if (editing?.row === index) {
			editing = null
			draft = ''
		}
		if (selectedCell?.row === index) selectedCell = null
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
			{ id: 'insert', label: 'Copy as INSERT' },
			{ id: 'update', label: 'Copy as UPDATE' },
			{ id: 'in', label: 'Copy as IN list' },
			{ id: 'where', label: 'Copy as WHERE' },
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
		if (
			id === 'copy' ||
			id === 'csv' ||
			id === 'markdown' ||
			id === 'json' ||
			id === 'insert' ||
			id === 'update' ||
			id === 'in' ||
			id === 'where'
		) {
			void copyRowAs(targets, id)
			return
		}
		if (id === 'clone') {
			void cloneRows(targets)
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
				label: ro ? 'View in modal' : 'Edit Modal',
				shortcut: '⇧Enter',
				disabled: pending || cellCount > 1,
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
			{ id: 'copy-row-insert', label: 'Copy row as INSERT' },
			{ id: 'copy-row-update', label: 'Copy row as UPDATE' },
			{ id: 'copy-row-in', label: 'Copy as IN list' },
			{ id: 'copy-row-where', label: 'Copy as WHERE' },
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
				showToast('danger', 'Could not read clipboard')
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
		if (id === 'copy-row-insert') {
			void copyRowAs(targetRows(row), 'insert')
			return
		}
		if (id === 'copy-row-update') {
			void copyRowAs(targetRows(row), 'update')
			return
		}
		if (id === 'copy-row-in') {
			void copyRowAs(targetRows(row), 'in')
			return
		}
		if (id === 'copy-row-where') {
			void copyRowAs(targetRows(row), 'where')
			return
		}
		if (id === 'copy-column') {
			const res = await copyTextToClipboard(col)
			notifyCopy(res, 'Column name copied')
			return
		}
		if (id === 'clone') {
			void cloneRows(targetRows(row))
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

	function seedSignature(): string {
		const filters = (seedFilters ?? [])
			.map((f) => `${f.column}:${f.op}:${f.value}`)
			.join('|')
		const code = seedCodeFilter?.trim() ?? ''
		const sort =
			seedSort?.column && seedSort.dir
				? `${seedSort.column}:${seedSort.dir}`
				: ''
		return `${code}\n${filters}\n${sort}`
	}

	/** What the parent would persist if it reloaded from the current filter bar. */
	function localSeedSignature(): string {
		const code = filtersActive && filterMode === 'code' ? filterCode.trim() : ''
		const filters =
			!filtersActive || filterMode === 'code'
				? ''
				: activeRules()
						.map((r) => `${r.column}:${r.op}:${r.value}`)
						.join('|')
		const sort = sortColumn && sortDir ? `${sortColumn}:${sortDir}` : ''
		return `${code}\n${filters}\n${sort}`
	}

	function applySeedState() {
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
		appliedSeedSig = seedSignature()
	}

	$effect(() => {
		const id = table.id
		if (loadedTableId === id) return
		const key = tabId
		if (key) {
			const existing = getEditBuffer(key)
			if (shouldRestoreEditBuffer(existing, table.rows.length)) {
				restoreBuffer(existing)
				applySeedState()
				appliedSyncTick = rowSyncTick
				appliedPreviewTick = tablePreviewTick
				return
			}
		}
		loadFromTable(true)
		page = 0
		applySeedState()
		appliedSyncTick = -1
		appliedPreviewTick = tablePreviewTick
	})

	$effect(() => {
		const key = tabId
		const ready = loadedTableId
		if (!key || ready !== table.id) return
		const snap = {
			baseline: originals.map((r) => ({ ...r })),
			rows: rows.map((r) => ({ ...r })),
			dirtyKeys: [...dirtyKeys],
			newRowIndices: [...newRowIndices],
			deletedIndices: [...deletedIndices],
			page,
		}
		untrack(() => rememberEditBuffer(key, snap))
	})

	// FK jump (or explorer re-open) can reseed the same table without remounting.
	$effect(() => {
		const sig = seedSignature()
		if (loadedTableId !== table.id) return
		if (sig === appliedSeedSig) return
		// Filter apply writes the same seeds we already have in the bar —
		// don't rebuild rules (that would drop extra empty conditions).
		if (sig === localSeedSignature()) {
			appliedSeedSig = sig
			return
		}
		applySeedState()
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

	// Filter / sort / page / open replace the preview page. That is a new
	// saved baseline, not an in-place inspector edit.
	$effect(() => {
		const tick = tablePreviewTick
		const tableId = table.id
		if (loadedTableId !== tableId) return
		if (tick === appliedPreviewTick) return
		if (editing) return
		if (tabId && tabEditsAreDirty(tabId)) {
			appliedPreviewTick = tick
			return
		}
		appliedPreviewTick = tick
		loadFromTable(true)
		appliedSyncTick = rowSyncTick
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
		const scroller = gridScrollEl()
		if (!scroller) return
		const colIdx = visible.names.indexOf(col)
		const rowH = appPrefs.gridRowHeightPx
		const headerH =
			scroller.querySelector('[data-grid-head]')?.getBoundingClientRect()
				.height ?? 36
		const top = row * rowH
		const viewTop = scroller.scrollTop
		const viewSpan = Math.max(0, scroller.clientHeight - headerH)
		if (top < viewTop) scroller.scrollTop = top
		else if (top + rowH > viewTop + viewSpan) {
			scroller.scrollTop = Math.max(0, top + rowH - viewSpan)
		}
		// Frozen columns never scroll sideways.
		if (colIdx >= 0 && colIdx < visible.frozen) return
		// First scrolling column: snap fully left so the # gutter is not left
		// off-screen after scrolling in from the right. Geometry, not the cell
		// node — a windowed cell is not mounted until this scroll lands.
		if (colIdx <= visible.frozen) {
			pinScrollToRowGutter()
			return
		}
		// Columns have their own widths, so ask the table where this one sits.
		const box = gridTable?.columnBox(col)
		if (!box || box.frozen) return
		const pinnedW = box.pinned
		const left = box.left
		const right = left + box.width
		const viewLeft = scroller.scrollLeft
		const viewRight = viewLeft + scroller.clientWidth
		if (left < viewLeft + pinnedW) scroller.scrollLeft = Math.max(0, left - pinnedW)
		else if (right > viewRight) scroller.scrollLeft = right - scroller.clientWidth
	}

	/** Current page is already fetched from the server; just scroll. */
	function ensureRowVisible(absoluteRow: number) {
		const col = selectedCell?.col ?? visible.names[0]
		if (col) scrollCellIntoView(absoluteRow, col)
	}

	function moveSelection(dRow: number, dCol: number, extend = false) {
		const nextCol = stepColumn(visible, selectedCell?.col, dCol)
		if (!nextCol || pageEntries.length === 0) return

		let row =
			selectedCell?.row ??
			highlightRow ??
			selectionAnchor ??
			pageEntries[0]!.index

		// Map absolute row → position in the current engine page
		let filterIdx = pageEntries.findIndex((e) => e.index === row)
		if (filterIdx < 0) filterIdx = 0

		filterIdx = Math.max(0, Math.min(pageEntries.length - 1, filterIdx + dRow))

		const nextAbs = pageEntries[filterIdx]!.index
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
		if (!assertWritable()) return
		if (deletedIndices.has(row)) {
			flashStatus('Row is marked for delete — undo delete to edit')
			return
		}
		if (isLargeCell(rows[row]?.[col])) {
			if (seedDraft !== undefined) {
				flashStatus('Open the cell to load the full value before editing.')
				return
			}
			openCellModal(row, col)
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

	function currentSnapshot(): GridEditSnapshot {
		return {
			rows: rows.map((row) => ({ ...row })),
			originals: originals.map((row) => ({ ...row })),
			dirtyKeys: [...dirtyKeys],
			newRows: [...newRowIndices],
			deleted: [...deletedIndices],
		}
	}

	function noteEdit() {
		if (restoringHistory) return
		undoStack = pushUndo(undoStack, currentSnapshot())
		redoStack = []
	}

	function applySnapshot(snapshot: GridEditSnapshot) {
		restoringHistory = true
		const next = cloneSnapshot(snapshot)
		rows = next.rows
		originals = next.originals
		dirtyKeys = new Set(next.dirtyKeys)
		newRowIndices = new Set(next.newRows)
		deletedIndices = new Set(next.deleted)
		restoringHistory = false
		ondirtychange?.(
			dirtyKeys.size > 0 || newRowIndices.size > 0 || deletedIndices.size > 0,
		)
	}

	function undoPending() {
		const step = undoEdit(undoStack, redoStack, currentSnapshot())
		if (!step) return
		undoStack = step.undo
		redoStack = step.redo
		applySnapshot(step.current)
	}

	function redoPending() {
		const step = redoEdit(undoStack, redoStack, currentSnapshot())
		if (!step) return
		undoStack = step.undo
		redoStack = step.redo
		applySnapshot(step.current)
	}

	function draftToValue(row: number, col: string, rawText: string): unknown {
		const prev = rows[row]?.[col]
		const def = columnDef(col)
		return def && usesSelectEditor(def, prev)
			? selectDraftToValue(rawText, def, prev)
			: parseDraft(rawText, prev)
	}

	function applyCellValue(row: number, col: string, rawText: string, track = true) {
		if (!assertWritable()) return
		if (track) noteEdit()
		const next = draftToValue(row, col, rawText)
		rows = rows.map((r, i) => (i === row ? { ...r, [col]: next } : r))
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
		const large = isLargeCell(value) ? value : null
		cellModal = {
			row,
			col,
			type: def?.type ?? '',
			text: seedText !== undefined ? seedText : valueToEditorText(value),
			locator: large?.locator ?? null,
			unfetchable: !!large && !(large.locator && large.locator.keys.length > 0),
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

	async function copySelectedCell(e?: ClipboardEvent) {
		if (!selectedCell || editing) return
		const v = rows[selectedCell.row]?.[selectedCell.col]
		const partial = isLargeCell(v)
		// Clipboard: empty string for NULL (paste-friendly). A large cell
		// copies its preview until the modal has loaded the rest.
		const text =
			v === null || v === undefined
				? ''
				: partial
					? largeCellText(v)
					: cellText(v)
		const message = partial ? `Cell copied. ${PARTIAL_COPY_MESSAGE}` : 'Cell copied'
		if (e?.clipboardData) {
			e.preventDefault()
			e.clipboardData.setData('text/plain', text)
			if (partial) showToast('warning', message)
			else showStatus(message)
			return
		}
		notifyCopy(await copyTextToClipboard(text), message, partial)
	}

	function pasteIntoGrid(text: string) {
		if (!assertWritable()) return
		const cols = visible.names
		if (cols.length === 0) return

		const lines = text.replace(/\r\n/g, '\n').replace(/\r/g, '\n').split('\n')
		if (lines.length > 1 && lines[lines.length - 1] === '') lines.pop()
		if (lines.length === 0) return

		const parsed = lines.map((line) => line.split('\t'))
		const firstLine = parsed[0]
		const isScalar = parsed.length === 1 && (firstLine?.length ?? 0) === 1
		const targets = actionCells().filter((c) => !deletedIndices.has(c.row))
		const edits: CellEdit[] = []

		// One clipboard value → fill every selected cell (or the focused cell).
		if (isScalar && targets.length > 0) {
			const value = firstLine?.[0] ?? ''
			for (const { row, col } of targets) {
				edits.push({ row, col, value: draftToValue(row, col, value) })
			}
		} else {
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
			if (!cols.includes(start.col)) return

			let filterIdx = pageEntries.findIndex((e) => e.index === start.row)
			if (filterIdx < 0) filterIdx = 0

			for (let r = 0; r < parsed.length; r++) {
				const cells = parsed[r]
				const entry = pageEntries[filterIdx + r]
				if (!cells || !entry) break
				const absRow = entry.index
				if (deletedIndices.has(absRow)) continue
				// Visible columns only: a hidden column is skipped, never written.
				columnsFrom(visible, start.col, cells.length).forEach((colName, c) => {
					edits.push({
						row: absRow,
						col: colName,
						value: draftToValue(absRow, colName, cells[c] ?? ''),
					})
				})
			}
		}
		if (edits.length === 0) return
		noteEdit()
		dirtyKeys = applyCellBatch(rows, originals, dirtyKeys, edits, (next) => {
			rows = next
		})
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
			showStatus('Save or reset changes before changing page.', 'warning')
			return
		}
		const p = Math.max(0, Math.min(totalPages - 1, nextPage))
		if (p === safePage) return
		page = p
		if (!onreload) return
		refreshing = true
		try {
			await onreload(reloadOpts(p * PAGE_SIZE))
			loadFromTable(true)
			markServerPreviewApplied()
		} catch (err) {
			reportGridError(err)
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
		if (dirtyCount > 0 && !refreshArmed) {
			refreshArmed = true
			showStatus('Unsaved edits. Refresh again to discard them.', 'warning')
			return
		}
		refreshArmed = false
		const keepPage = page
		refreshing = true
		try {
			if (onreload) {
				await onreload(reloadOpts(keepPage * PAGE_SIZE))
				loadFromTable(true)
				markServerPreviewApplied()
			} else {
				const cloned = seed.map((r) => ({ ...r }))
				rows = cloned.map((r) => ({ ...r }))
				originals = cloned.map((r) => ({ ...r }))
				dirtyKeys = new Set()
				newRowIndices = new Set()
				deletedIndices = new Set()
				if (tabId) adoptServerRows(tabId, cloned, keepPage)
			}
			page = keepPage
			onbaselinechange?.()
		} catch (err) {
			reportGridError(err)
		} finally {
			refreshing = false
		}
	}

	function editPayload(sid: string, dryRun: boolean) {
		return {
			sessionId: sid,
			engine,
			table,
			rows,
			originals,
			newRowIndices,
			deletedIndices,
			dirtyKeys,
			profileReadOnly,
			dryRun,
		}
	}

	async function liveSessionId(): Promise<string> {
		let sid = sessionId
		if (!sid && onreconnect) sid = await onreconnect()
		return sid ?? ''
	}

	/** Waiting close/import prompt. Settled when the save commits, fails, or is cancelled. */
	let saveWaiter: ((outcome: SaveOutcome) => void) | null = null

	function settleSave(outcome: SaveOutcome) {
		const waiter = saveWaiter
		saveWaiter = null
		waiter?.(outcome)
	}

	function saveForPrompt(): Promise<SaveOutcome> {
		settleSave({ ok: false, error: null })
		return new Promise((resolve) => {
			saveWaiter = resolve
			void saveChanges()
		})
	}

	function failSave(err: unknown) {
		reportGridError(err)
		settleSave({ ok: false, error: err instanceof Error ? err.message : String(err) })
	}

	/** "Review before saving: always": the one dialog, shown before the save. */
	async function reviewSave(sid: string): Promise<'save' | 'empty' | 'cancel'> {
		const planned = await persistTableEdits(editPayload(sid, true))
		const statements = planned.statements ?? []
		if (statements.length === 0) return 'empty'
		const decision = await askWriteGuard(
			reviewPrompt({ production, connectionName, statements }),
		)
		if (!decision.ok) return 'cancel'
		// Production: this dialog is also the unlock, so the backend asks nothing more.
		if (production) await unlockEngineWrites(sid)
		return 'save'
	}

	/** Saves straight away. Production asks once per session (the backend guard). */
	async function saveChanges() {
		if (saving) return
		if (!assertWritable()) {
			settleSave({ ok: false, error: 'Connection is read-only' })
			return
		}
		if (editing) commitEdit()
		if (dirtyCount === 0) {
			settleSave({ ok: true })
			return
		}
		saving = true
		try {
			const sid = await liveSessionId()
			if (reviewsEverySave(appPrefs.reviewBeforeSaving)) {
				const step = await reviewSave(sid)
				if (step !== 'save') {
					settleSave(step === 'empty' ? { ok: true } : { ok: false, error: null })
					return
				}
			}
			const pendingDbDeletes = [...deletedIndices].filter(
				(i) => !newRowIndices.has(i),
			).length
			// Do not retry after a dropped connection: the save may have landed.
			const result = await persistTableEdits(editPayload(sid, false))
			if (pendingDbDeletes > 0 && (result.deleted ?? 0) < pendingDbDeletes) {
				throw new Error(
					'Delete did not apply. Restart Mystique and try the delete again.',
				)
			}
			lastSaveId = result.saveId ?? null
			undoStack = []
			redoStack = []
			if (onreload) {
				await onreload(reloadOpts(page * PAGE_SIZE))
				loadFromTable(true)
				markServerPreviewApplied()
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
			const saveId = result.saveId
			showToast(
				'success',
				savedMessage(result),
				saveId
					? { action: { label: 'Undo', run: () => rollbackSave(saveId) } }
					: undefined,
			)
			onbaselinechange?.()
			settleSave({ ok: true })
		} catch (err) {
			if (isWriteCancelled(err)) settleSave({ ok: false, error: null })
			else failSave(err)
		} finally {
			saving = false
		}
	}

	const sqlPanel = latestRun({
		delayMs: 300,
		run: async () => {
			const sid = sessionId ?? ''
			if (dirtyCount === 0 || !sid) return []
			const planned = await persistTableEdits(editPayload(sid, true))
			return planned.statements ?? []
		},
		onResult: (statements) => {
			sqlPanelStatements = statements
			sqlPanelError = null
		},
		onError: (err) => {
			sqlPanelError = err instanceof Error ? err.message : String(err)
		},
	})

	$effect(() => {
		if (!appPrefs.gridSqlPanel) return
		// Replan whenever the pending edits change.
		void rows
		void dirtyKeys
		void newRowIndices
		void deletedIndices
		sqlPanel.schedule()
		return () => sqlPanel.cancel()
	})

	const columnMenuItems = $derived.by((): ContextMenuItem[] => {
		const name = columnMenu?.column
		if (!name) return []
		return [
			{
				id: 'freeze',
				label: columnLayout.frozen.includes(name) ? 'Unfreeze column' : 'Freeze column',
			},
			{ id: 'hide', label: 'Hide column' },
			{ id: 'left', label: 'Move left' },
			{ id: 'right', label: 'Move right' },
		]
	})

	function changeLayout(next: GridColumnLayout) {
		columnLayout = next
		if (!connectionId) return
		void saveColumnLayout(connectionId, table.id, next).catch(() => {})
	}

	function onColumnMenuPick(id: string) {
		const name = columnMenu?.column
		columnMenu = null
		if (!name) return
		const names = table.columns.map((column) => column.name)
		if (id === 'freeze') changeLayout(toggleFrozen(columnLayout, name))
		if (id === 'hide') changeLayout(toggleHidden(columnLayout, name))
		if (id === 'left') changeLayout(moveColumn(columnLayout, names, name, -1))
		if (id === 'right') changeLayout(moveColumn(columnLayout, names, name, 1))
	}

	let loadedLayoutKey = ''
	$effect(() => {
		const id = connectionId
		const tableId = table.id
		const key = `${id ?? ''}\n${tableId}`
		if (!id || loadedLayoutKey === key) return
		loadedLayoutKey = key
		undoStack = []
		redoStack = []
		void loadColumnLayout(id, tableId)
			.then((layout) => {
				if (loadedLayoutKey !== key) return
				// No stored layout: start clean, not with the last table's widths.
				columnLayout = layout ?? { ...EMPTY_COLUMN_LAYOUT, widths: {} }
			})
			.catch(() => {})
		lastSaveId = null
		refreshLatestSave(key)
	})

	function refreshLatestSave(key = loadedLayoutKey) {
		const id = connectionId
		if (!id) return
		void loadLatestSaveId(id, table)
			.then((saveId) => {
				if (loadedLayoutKey !== key) return
				lastSaveId = saveId
			})
			.catch(() => {})
	}

	/** Undo one save by its backend id (Rollback button and the Undo toast). */
	async function rollbackSave(saveId: string) {
		if (saving) return
		if (!assertWritable()) return
		saving = true
		try {
			const sid = await liveSessionId()
			const outcome = await undoSave(
				() => rollbackTableEdits({ sessionId: sid, saveId, profileReadOnly }),
				isWriteCancelled,
			)
			// A stale or used save is gone on the backend; offer the next one.
			refreshLatestSave()
			if (!outcome.ok) {
				if (outcome.message) showToast('danger', outcome.message)
				return
			}
			if (lastSaveId === saveId) lastSaveId = null
			undoStack = []
			redoStack = []
			if (onreload) {
				await onreload(reloadOpts(page * PAGE_SIZE))
				loadFromTable(true)
				markServerPreviewApplied()
			}
			showToast('success', 'Rolled back the save')
			onbaselinechange?.()
		} catch (err) {
			reportGridError(err)
		} finally {
			saving = false
		}
	}

	/** Grid chords stay out of text fields other than the cell being edited. */
	function gridKeysAllowed({ focus }: CommandContext): boolean {
		const target = focus?.target instanceof HTMLElement ? focus.target : null
		const inCellEditor = !!(
			editing &&
			target &&
			(target === inputEl || inputEl?.contains(target))
		)
		return !gridShortcutBlocked({
			modalOpen: cellModal != null,
			inCellEditor,
			tagName: target?.tagName,
			contentEditable: target?.isContentEditable,
			inDialog: !!target?.closest('[role="dialog"]'),
		})
	}

	// Table commands and File→Save while this table is the active tab.
	onMount(() => {
		const unregisterCommands = commands.register([
			{ ...cmd.save, run: () => void saveChanges() },
			{ ...cmd.gridNewRow, when: gridKeysAllowed, run: () => addNewRow() },
			{
				...cmd.gridDuplicateRow,
				when: gridKeysAllowed,
				run: () => cloneFocusedRow(),
			},
			// Instead of reloading the page.
			{ ...cmd.gridRefresh, when: gridKeysAllowed, run: () => void refresh() },
			{ ...cmd.gridUndo, when: gridKeysAllowed, run: () => undoPending() },
			{ ...cmd.gridRedo, when: gridKeysAllowed, run: () => redoPending() },
		])
		const unregisterSaver = tabId ? registerTabSaver(tabId, saveForPrompt) : null
		return () => {
			unregisterCommands()
			unregisterSaver?.()
			settleSave({ ok: false, error: null })
		}
	})

	function resetChanges() {
		if (editing) cancelEdit()
		if (dirtyCount === 0) return
		noteEdit()

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
		onbaselinechange?.()
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
		completions={codeCompletions}
		onCodeInput={onCodeInput}
		onPickCompletion={pickCompletion}
		distinctValues={supportsDistinct ? distinctValues : []}
	/>

	<div class="flex min-h-0 min-w-0 flex-1">
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
			bind:this={gridTable}
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
			{columnLayout}
			onColumnMenu={(column, event) => {
				columnMenu = { column, x: event.clientX, y: event.clientY }
			}}
			onColumnWidth={(column, width) =>
				changeLayout(withColumnWidth(columnLayout, column, width))}
		/>
	</div>
	{#if appPrefs.gridSqlPanel}
		<GridSqlPanel
			statements={sqlPanelStatements}
			error={sqlPanelError}
			onclose={() => appPrefs.setGridSqlPanel(false)}
		/>
	{/if}
	</div>

	{#if cellModal}
		{@const locator = cellModal.locator}
		<CellValueModal
			column={cellModal.col}
			columnType={cellModal.type}
			rowIndex={cellModal.row}
			initialText={cellModal.text}
			readonly={profileReadOnly}
			unfetchable={cellModal.unfetchable || (!!locator && !sessionId)}
			fetchChunk={locator && sessionId
				? (offset) => fetchEngineLargeValue({ sessionId, locator, offset })
				: undefined}
			onapply={applyCellModal}
			onclose={closeCellModal}
		/>
	{/if}

	{#if columnMenu}
		<ContextMenu
			x={columnMenu.x}
			y={columnMenu.y}
			items={columnMenuItems}
			ariaLabel="Column actions"
			onpick={onColumnMenuPick}
			onclose={() => (columnMenu = null)}
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
		{dirtyCount}
		{lastFetchedAt}
		{safePage}
		{totalPages}
		{refreshing}
		{saving}
		readOnly={profileReadOnly}
		onOpenStructure={onopenstructure}
		onPrev={goPrev}
		onNext={goNext}
		onRefresh={() => void refresh()}
		onAddRow={addNewRow}
		onReset={resetChanges}
		onSave={() => void saveChanges()}
		{selectionLabel}
		canUndo={undoStack.length > 0}
		canRedo={redoStack.length > 0}
		canRollback={lastSaveId != null && !profileReadOnly}
		onUndo={undoPending}
		onRedo={redoPending}
		onRollback={() => {
			if (lastSaveId) void rollbackSave(lastSaveId)
		}}
		sqlPanelOpen={appPrefs.gridSqlPanel}
		onToggleSql={() => appPrefs.setGridSqlPanel(!appPrefs.gridSqlPanel)}
	/>
</div>
