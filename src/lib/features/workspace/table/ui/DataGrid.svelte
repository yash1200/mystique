<script lang="ts">
	/**
	 * The table tab's grid: composition only. State lives in the table stores
	 * (`TableEdits`, `TableFilters`, `TablePaging`, `TableSaver`, `CellEditor`,
	 * `TableColumns`, `TableLoader`, `TableRowActions`) and the shared
	 * `GridSelection`; this component wires them to pointer, keys and markup.
	 */
	import type {
		ColumnReference,
		TableMeta,
		TablePreviewFilter,
		TablePreviewSortDir,
	} from '#core/contracts'
	import { commands, type CommandContext } from '#core/commands'
	import { ContextMenu } from '#core/ui'
	import { errorMessage, fetchEngineLargeValue } from '#core/ipc'
	import { showStatus, showToast } from '#features/toast'
	import {
		CellValueModal,
		formatSelectionStats,
		GridSelection,
		GridTable,
		gridKeyIntent,
		gridShortcutBlocked,
		isTypingTarget,
		selectionStats,
		usesSelectEditor,
		type GridKeyIntent,
	} from '#features/workspace/data-grid'
	import { registerTabSaver } from '#features/workspace/session'
	import { appPrefs, cmd } from '#lib/app'
	import { onMount, tick } from 'svelte'
	import { cellFilterMenu, cellFilterPick, cellFilterValue, type CellFilterOp } from '../cell-filter'
	import { TableColumns } from '../columns.svelte'
	import { countLabel } from '../count'
	import type { TableCopySource } from '../copy'
	import { CellEditor } from '../editing.svelte'
	import { TableEdits } from '../edits.svelte'
	import { TableFilters } from '../filters/filter-state.svelte'
	import { TableLoader } from '../loader.svelte'
	import { TablePaging } from '../paging.svelte'
	import { TableRowActions } from '../row-actions'
	import { TableSaver } from '../save.svelte'
	import { tablePreview } from '../tab-preview.svelte'
	import { pinToRowGutter, scrollCellIntoView } from '../scroll'
	import GridFilterBar from './GridFilterBar.svelte'
	import GridSqlPanel from './GridSqlPanel.svelte'
	import GridStatusBar from './GridStatusBar.svelte'

	interface Props {
		table: TableMeta
		selectedRowIndex: number | null
		onselectrow: (index: number) => void
		/** Notify parent when unsaved edit count changes (for tab dirty indicator). */
		ondirtychange?: (dirty: boolean) => void
		/** Last saved row snapshots — bound so the inspector can mark dirty fields. */
		originals?: Record<string, unknown>[]
		engine?: string
		sessionId?: string | null
		readOnly?: boolean
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
		ondirtychange,
		originals = $bindable<Record<string, unknown>[]>([]),
		engine = '',
		sessionId = null,
		readOnly = false,
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

	let gridShellEl = $state<HTMLDivElement | null>(null)
	let gridTable = $state<ReturnType<typeof GridTable> | null>(null)
	let rowMenu = $state<{ row: number; x: number; y: number } | null>(null)
	let cellMenu = $state<{ row: number; col: string; x: number; y: number } | null>(null)

	const columns = new TableColumns({ table: () => table, connectionId: () => connectionId })
	const sel = new GridSelection<string>(() => columns.visible.names)
	const edits = new TableEdits({
		columns: () => table.columns,
		baseline: { get: () => originals, set: (rows) => (originals = rows) },
	})
	const filters = new TableFilters({
		table: () => table,
		sessionId: () => sessionId,
		supportsDistinct: () => supportsDistinct,
	})
	const paging = new TablePaging({ table: () => table, loaded: () => edits.rows.length })
	const editor = new CellEditor({
		edits,
		sel,
		column: (name) => table.columns.find((c) => c.name === name),
		writable: assertWritable,
		flash: flashStatus,
		focusGrid: focusGridShell,
		move: (dRow, dCol) => moveSelection({ dRow, dCol, extend: false }),
	})
	const saver = new TableSaver({
		edits,
		table: () => table,
		engine: () => engine,
		connectionId: () => connectionId,
		sessionId: () => sessionId,
		reconnect: () => onreconnect,
		production: () => production,
		connectionName: () => connectionName,
		writable: assertWritable,
		commitEdit: () => editor.commit(),
		reload: () => loader.reloadPage(),
		adoptSaved: () => loader.replaceTableRows(edits.adoptSaved()),
		baselineChanged: () => loader.baselineChanged(),
		report: reportGridError,
	})
	const loader = new TableLoader({
		table: () => table,
		tabId: () => tabId,
		connectionId: () => connectionId,
		onreload: () => onreload,
		seed: () => ({ filters: seedFilters, codeFilter: seedCodeFilter, sort: seedSort }),
		edits,
		filters,
		paging,
		columns,
		sel,
		editor,
		forTable: (key) => saver.forTable(key),
		flash: flashStatus,
		report: reportGridError,
	})
	const copySource: TableCopySource = {
		get table() {
			return table
		},
		get rows() {
			return edits.rows
		},
		get engine() {
			return engine
		},
		get sqlDialect() {
			return sqlDialect
		},
	}
	const actions = new TableRowActions({
		table: () => table,
		sessionId: () => sessionId,
		readOnly: () => readOnly,
		inspectedRow: () => selectedRowIndex,
		edits,
		sel,
		editor,
		copy: copySource,
		visibleColumns: () => columns.visible.names,
		writable: assertWritable,
		flash: flashStatus,
		report: reportGridError,
		follow: (row) => {
			if (shouldOpenInspector()) onselectrow(row)
		},
		inspect: (row) => onselectrow(row),
		reveal: (row, col) =>
			void tick().then(() => {
				scrollTo(row, col)
				focusGridShell()
			}),
	})

	/** Current engine page (already filtered server-side). */
	const pageEntries = $derived(edits.rows.map((row, index) => ({ row, index })))
	const selectionLabel = $derived(
		formatSelectionStats(
			selectionStats(
				sel.values(
					(r, c) => edits.rows[r]?.[c],
					(r) => {
						const row = edits.rows[r]
						if (!row) return null
						return table.columns.filter((c) => !columns.isHidden(c.name)).map((c) => row[c.name])
					},
				),
			),
		),
	)
	const rowCount = $derived(tabId ? countLabel(tablePreview.countByTab[tabId]) : null)
	const rowMenuItems = $derived(rowMenu ? actions.rowMenu(rowMenu.row) : [])
	const cellMenuItems = $derived.by(() => {
		if (!cellMenu) return []
		const rest = actions.cellMenu(cellMenu.row)
		const filterItems = cellFilterMenu(cellMenu.col, cellFilterFor(cellMenu.row, cellMenu.col))
		return [
			...filterItems,
			...rest.map((item, i) => (i === 0 ? { ...item, separatorBefore: true } : item)),
		]
	})

	function cellFilterFor(row: number, col: string) {
		const type = table.columns.find((c) => c.name === col)?.type
		return cellFilterValue(edits.rows[row]?.[col], type, engine)
	}

	/** "Filter = value" and friends: add the rule to the bar and reload. */
	function filterByCell(row: number, col: string, op: CellFilterOp) {
		const value = cellFilterFor(row, col)
		if (op === 'eq' || op === 'neq') {
			if (value.kind !== 'value') return
		} else if (value.kind !== 'null') {
			return
		}
		const problem = filters.addCellRule(col, op, value.kind === 'value' ? value.text : '')
		if (problem) {
			flashStatus(problem)
			return
		}
		loader.applyFilters()
	}

	$effect(() => {
		ondirtychange?.(edits.dirtyCount > 0)
	})

	function flashStatus(msg: string) {
		showStatus(msg, 'warning')
	}

	function reportGridError(err: unknown) {
		showToast('danger', errorMessage(err))
	}

	function assertWritable(): boolean {
		if (!readOnly) return true
		flashStatus('Connection is read-only')
		return false
	}

	function focusGridShell() {
		// Defer so we don't steal focus mid-edit open.
		queueMicrotask(() => {
			if (!editor.editing) gridShellEl?.focus({ preventScroll: true })
		})
	}

	function shouldOpenInspector(): boolean {
		// Pref default on. If the panel is already open, always follow the row.
		return appPrefs.inspectOnRowClick || selectedRowIndex != null
	}

	function scrollTo(row: number, col: string) {
		scrollCellIntoView({
			shell: gridShellEl,
			visible: columns.visible,
			rowH: appPrefs.gridRowHeightPx,
			columnBox: (name) => gridTable?.columnBox(name),
			row,
			col,
		})
	}

	// ── Pointer and keys ────────────────────────────────────────────────

	function selectCell(row: number, col: string, e?: MouseEvent) {
		const editing = editor.editing
		if (editing) {
			if (editing.row === row && editing.col === col) return
			editor.commit()
		}
		sel.pointCell(row, col, e)
		if (shouldOpenInspector()) onselectrow(row)
		focusGridShell()
		scrollTo(row, col)
	}

	function selectRowOnly(row: number, e?: MouseEvent) {
		if (editor.editing) editor.commit()
		sel.pointRow(row, e)
		sel.setCell(null)
		if (shouldOpenInspector()) onselectrow(row)
		focusGridShell()
		pinToRowGutter(gridShellEl)
	}

	function moveSelection(step: { dRow: number; dCol: number; extend: boolean }) {
		const fromCol = sel.cell?.col ?? columns.visible.names[0]
		const moved = sel.move(step, { first: 0, count: edits.rows.length })
		if (!moved) return
		if (fromCol) scrollTo(moved.row, fromCol)
		// Keep the row details panel in sync (same path as click selection).
		if (shouldOpenInspector()) onselectrow(moved.row)
		// Wait a frame for page change DOM
		requestAnimationFrame(() => scrollTo(moved.row, moved.col))
	}

	function openRowMenu(row: number, e: MouseEvent) {
		cellMenu = null
		if (sel.kind !== 'rows' || !sel.rows.has(row)) sel.selectRow(row)
		else sel.focusRow(row)
		rowMenu = { row, x: e.clientX, y: e.clientY }
	}

	function openCellMenu(row: number, col: string, e: MouseEvent) {
		rowMenu = null
		if (!sel.isCellSelected(row, col)) sel.selectCell(row, col)
		else sel.focus(row, col)
		cellMenu = { row, col, x: e.clientX, y: e.clientY }
	}

	/** True when the grid handled the intent (and the default is prevented). */
	function runKeyIntent(intent: GridKeyIntent): boolean {
		const cell = sel.cell
		switch (intent.kind) {
			case 'copy':
				actions.copy()
				return true
			case 'edit':
				if (cell) editor.start(cell.row, cell.col)
				return !!cell
			case 'expand':
				if (cell) editor.openModal(cell.row, cell.col)
				return !!cell
			case 'delete-rows':
				return actions.toggleDelete()
			case 'set-null': {
				const cells = sel.actionCells()
				if (cells.length > 0) actions.setCellsNull(cells)
				return cells.length > 0
			}
			case 'escape':
				if (!sel.collapse() && cell) sel.clearCells()
				return true
			case 'select-all':
				sel.selectAll(pageEntries.map((entry) => entry.index))
				return true
			case 'move':
				moveSelection(intent)
				return true
			case 'type': {
				if (!cell) return false
				// Selects: open editor so user picks from list
				const def = table.columns.find((c) => c.name === cell.col)
				const prev = edits.rows[cell.row]?.[cell.col]
				if (def && usesSelectEditor(def, prev)) editor.start(cell.row, cell.col)
				else editor.start(cell.row, cell.col, undefined, intent.key)
				return true
			}
		}
	}

	function onGridKeydown(e: KeyboardEvent) {
		if (editor.modal) return
		const mod = e.metaKey || e.ctrlKey
		// ⌘/Ctrl+S save, +N new row, +D duplicate — also while a cell is being edited.
		if (mod && !e.altKey && !e.shiftKey) {
			const key = e.key.toLowerCase()
			const run =
				key === 's'
					? () => void saver.save()
					: key === 'n'
						? () => actions.addRow()
						: key === 'd'
							? () => actions.duplicateFocused()
							: null
			if (run) {
				e.preventDefault()
				e.stopPropagation()
				run()
				return
			}
		}
		// Filter bar and other fields keep their keys; the cell input has its own handler.
		if (isTypingTarget(e.target) || editor.editing) return
		const intent = gridKeyIntent(e)
		if (intent && runKeyIntent(intent)) e.preventDefault()
	}

	function onGridCopy(e: ClipboardEvent) {
		if (!editor.editing) actions.copy(e)
	}

	function onGridPaste(e: ClipboardEvent) {
		if (editor.editing) return
		const text = e.clipboardData?.getData('text/plain')
		if (text == null) return
		e.preventDefault()
		actions.paste(text)
	}

	/** Grid chords stay out of text fields other than the cell being edited. */
	function gridKeysAllowed({ focus }: CommandContext): boolean {
		const target = focus?.target instanceof HTMLElement ? focus.target : null
		const input = editor.inputEl
		const inCellEditor = !!(
			editor.editing &&
			target &&
			(target === input || input?.contains(target))
		)
		return !gridShortcutBlocked({
			modalOpen: editor.modal != null,
			inCellEditor,
			tagName: target?.tagName,
			contentEditable: target?.isContentEditable,
			inDialog: !!target?.closest('[role="dialog"]'),
		})
	}

	function undoPending() {
		if (edits.undo()) ondirtychange?.(edits.hasMarks())
	}

	function redoPending() {
		if (edits.redo()) ondirtychange?.(edits.hasMarks())
	}

	// Table commands and File→Save while this table is the active tab.
	onMount(() => {
		const unregisterCommands = commands.register([
			{ ...cmd.save, run: () => void saver.save() },
			{ ...cmd.gridNewRow, when: gridKeysAllowed, run: () => actions.addRow() },
			{
				...cmd.gridDuplicateRow,
				when: gridKeysAllowed,
				run: () => actions.duplicateFocused(),
			},
			// Instead of reloading the page.
			{ ...cmd.gridRefresh, when: gridKeysAllowed, run: () => void loader.refresh() },
			{ ...cmd.gridUndo, when: gridKeysAllowed, run: () => undoPending() },
			{ ...cmd.gridRedo, when: gridKeysAllowed, run: () => redoPending() },
		])
		const unregisterSaver = tabId ? registerTabSaver(tabId, () => saver.saveForPrompt()) : null
		return () => {
			unregisterCommands()
			unregisterSaver?.()
			saver.dispose()
			filters.dispose()
		}
	})
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col">
	<GridFilterBar
		filterMode={filters.mode}
		filterRules={filters.rules}
		bind:filterCode={filters.code}
		columnOptions={filters.columnOptions}
		onSetMode={(mode) => filters.setMode(mode)}
		onUpdateFirst={(patch) => filters.updateFirst(patch)}
		onUpdateRule={(id, patch) => filters.updateRule(id, patch)}
		onRemoveRule={(id) => filters.removeRule(id)}
		onAddRule={() => filters.addRule()}
		onApply={() => loader.applyFilters()}
		onClear={() => loader.clearFilters()}
		codePlaceholder={codeFilterPlaceholder}
		completions={filters.completions}
		onCodeInput={(value, caret) => filters.onCodeInput(value, caret)}
		onPickCompletion={(item) => filters.pickCompletion(item)}
		valueSuggestions={filters.values.items}
		valueTarget={filters.valueTarget}
		onValueInput={(id) => filters.onRuleValueInput(id)}
		onPickValue={(value) => filters.pickValue(value)}
		onCloseValues={() => filters.closeValues()}
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
			selectedRowIndex={sel.kind === 'rows' ? (sel.row ?? selectedRowIndex) : null}
			isRowSelected={sel.isRowSelected}
			isCellSelected={sel.isCellSelected}
			selectedCell={sel.cell}
			{originals}
			editing={editor.editing}
			bind:draft={editor.draft}
			bind:inputEl={editor.inputEl}
			filtersActive={filters.active}
			hasActiveFilters={filters.hasActive}
			sortColumn={filters.sortColumn}
			sortDir={filters.sortDir}
			isDirty={edits.isDirty}
			isNewRow={edits.isNew}
			isDeletedRow={edits.isDeleted}
			onSelectCell={selectCell}
			onSelectRow={selectRowOnly}
			onStartEdit={(row, col, e) => editor.start(row, col, e)}
			onInputKeydown={editor.onInputKeydown}
			onCommitEdit={() => editor.commit()}
			onRowMenu={openRowMenu}
			onCellMenu={openCellMenu}
			onOpenReference={onopenreference}
			onSort={(column) => loader.cycleSort(column)}
			columnLayout={columns.layout}
			onColumnMenu={(column, event) => columns.openMenu(column, event)}
			onColumnWidth={(column, width) => columns.setWidth(column, width)}
		/>
	</div>
	{#if appPrefs.gridSqlPanel}
		<GridSqlPanel
			statements={saver.sqlStatements}
			error={saver.sqlError}
			onclose={() => appPrefs.setGridSqlPanel(false)}
		/>
	{/if}
	</div>

	{#if editor.modal}
		{@const modal = editor.modal}
		{@const locator = modal.locator}
		<CellValueModal
			column={modal.col}
			columnType={modal.type}
			rowIndex={modal.row}
			initialText={modal.text}
			readonly={readOnly}
			unfetchable={modal.unfetchable || (!!locator && !sessionId)}
			fetchChunk={locator && sessionId
				? (offset) => fetchEngineLargeValue({ sessionId, locator, offset })
				: undefined}
			onapply={(text) => editor.applyModal(text)}
			onclose={() => editor.closeModal()}
		/>
	{/if}

	{#if columns.menu}
		<ContextMenu
			x={columns.menu.x}
			y={columns.menu.y}
			items={columns.menuItems}
			ariaLabel="Column actions"
			onpick={(id) => columns.pickMenu(id)}
			onclose={() => (columns.menu = null)}
		/>
	{/if}

	{#if rowMenu}
		<ContextMenu
			x={rowMenu.x}
			y={rowMenu.y}
			items={rowMenuItems}
			ariaLabel="Row actions"
			onpick={(id) => {
				const row = rowMenu?.row
				rowMenu = null
				if (row != null) actions.pickRowMenu(row, id)
			}}
			onclose={() => (rowMenu = null)}
		/>
	{/if}

	{#if cellMenu}
		<ContextMenu
			x={cellMenu.x}
			y={cellMenu.y}
			items={cellMenuItems}
			ariaLabel="Cell actions"
			onpick={(id) => {
				const ctx = cellMenu
				cellMenu = null
				if (!ctx) return
				const filterOp = cellFilterPick(id)
				if (filterOp) filterByCell(ctx.row, ctx.col, filterOp)
				else actions.pickCellMenu(ctx.row, ctx.col, id)
			}}
			onclose={() => (cellMenu = null)}
		/>
	{/if}

	<GridStatusBar
		dirtyCount={edits.dirtyCount}
		lastFetchedAt={paging.lastFetchedAt}
		safePage={paging.safePage}
		totalPages={paging.totalPages}
		refreshing={paging.refreshing}
		saving={saver.saving}
		{readOnly}
		onOpenStructure={onopenstructure}
		onPrev={() => void loader.goToPage(paging.safePage - 1)}
		onNext={() => void loader.goToPage(paging.safePage + 1)}
		onRefresh={() => void loader.refresh()}
		onAddRow={() => actions.addRow()}
		onReset={() => loader.resetChanges()}
		onSave={() => void saver.save()}
		{selectionLabel}
		canUndo={edits.undoStack.length > 0}
		canRedo={edits.redoStack.length > 0}
		canRollback={saver.lastSaveId != null && !readOnly}
		onUndo={undoPending}
		onRedo={redoPending}
		onRollback={() => {
			if (saver.lastSaveId) void saver.rollback(saver.lastSaveId)
		}}
		sqlPanelOpen={appPrefs.gridSqlPanel}
		onToggleSql={() => appPrefs.setGridSqlPanel(!appPrefs.gridSqlPanel)}
		count={rowCount}
		onCountExactly={tabId ? () => void tablePreview.countExactly(tabId) : undefined}
		onCancelCount={tabId ? () => tablePreview.cancelCount(tabId) : undefined}
	/>
</div>
