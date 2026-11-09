/**
 * What the table grid does to rows and cells: add, duplicate, delete, set
 * NULL, copy and paste, and the row / cell context menus that run them.
 * Rows are page indices; every action goes through `TableEdits` so it can
 * be undone, and keeps the selection and the cell editor on the same rows.
 */
import type { TableMeta } from '#core/contracts'
import { fetchEngineLargeValue } from '#core/ipc'
import type { ContextMenuItem } from '#core/ui'
import { showStatus, showToast } from '#features/toast'
import { copyCells, type GridSelection } from '#features/workspace/data-grid'
import { cloneRowValues, type FetchChunk } from './clone-row'
import {
	copyTableCell,
	copyTableRows,
	copyTableSelection,
	type TableCopySource,
} from './copy'
import type { CellEditor } from './editing.svelte'
import type { TableEdits } from './edits.svelte'
import { tableCellMenu, tableCellPick, tableRowMenu, tableRowPick } from './menus'
import { pasteCellEdits } from './paste-batch'

type Row = Record<string, unknown>

export type RowActionsContext = {
	table: () => TableMeta
	sessionId: () => string | null
	readOnly: () => boolean
	/** Row the inspector shows (from the parent), the last fallback target. */
	inspectedRow: () => number | null
	edits: TableEdits
	sel: GridSelection<string>
	editor: CellEditor
	copy: TableCopySource
	/** Visible column order. */
	visibleColumns: () => readonly string[]
	/** False (after telling the user) on a read-only connection. */
	writable: () => boolean
	flash: (message: string) => void
	report: (err: unknown) => void
	/** The selection moved to a row: the inspector follows when it is set to. */
	follow: (row: number) => void
	/** Show details: open the inspector on this row. */
	inspect: (row: number) => void
	/** After the DOM updates, scroll this cell into view and focus the grid. */
	reveal: (row: number, col: string) => void
}

export class TableRowActions {
	readonly #ctx: RowActionsContext

	constructor(ctx: RowActionsContext) {
		this.#ctx = ctx
	}

	/** Rows an action applies to: the selection, else the focused row or cell, else the inspected row. */
	actionRows(): number[] {
		const { sel } = this.#ctx
		if (sel.kind === 'rows' && sel.rows.size > 0) return sel.rowList()
		if (sel.kind === 'cells' && sel.cells.size > 0) return sel.rowsInCells()
		if (sel.row != null) return [sel.row]
		if (sel.cell) return [sel.cell.row]
		const inspected = this.#ctx.inspectedRow()
		return inspected != null ? [inspected] : []
	}

	/** The menu row, or every action row when it is one of several. */
	targetRows(primary: number): number[] {
		const all = this.actionRows()
		return all.includes(primary) && all.length > 1 ? all : [primary]
	}

	#commitEdit() {
		if (this.#ctx.editor.editing) this.#ctx.editor.commit()
	}

	setCellsNull(cells: { row: number; col: string }[]) {
		const { edits, editor } = this.#ctx
		if (!this.#ctx.writable()) return
		this.#commitEdit()
		const pending = cells.filter(
			(cell) => !edits.isDeleted(cell.row) && edits.rows[cell.row]?.[cell.col] != null,
		)
		if (pending.length === 0) return
		edits.noteEdit()
		for (const { row, col } of pending) editor.apply(row, col, 'NULL', false)
	}

	/** Insert an unsaved row at the top of the grid and select it. */
	#insertTop(row: Row, track = true) {
		const { edits, editor, sel } = this.#ctx
		if (track) edits.noteEdit()
		editor.remap((i) => i + 1)
		sel.remap((i) => i + 1)
		edits.insertTop(row)
		const firstCol = this.#ctx.table().columns[0]?.name
		if (!firstCol) return
		sel.selectRow(0)
		sel.setCell({ row: 0, col: firstCol })
		this.#ctx.follow(0)
		this.#ctx.reveal(0, firstCol)
	}

	addRow() {
		if (!this.#ctx.writable()) return
		this.#commitEdit()
		const columns = this.#ctx.table().columns
		this.#insertTop(Object.fromEntries(columns.map((col) => [col.name, null])))
	}

	duplicateFocused() {
		const targets = this.actionRows()
		if (targets.length === 0) {
			this.#ctx.flash('Select a row to duplicate')
			return
		}
		void this.duplicate(targets)
	}

	/** Copies go on top as unsaved rows. Large values are fetched in full first. */
	async duplicate(indices: number[]) {
		const { edits } = this.#ctx
		if (!this.#ctx.writable()) return
		this.#commitEdit()
		const sid = this.#ctx.sessionId()
		const fetchChunk: FetchChunk | null = sid
			? (locator, offset) => fetchEngineLargeValue({ sessionId: sid, locator, offset })
			: null
		const columns = this.#ctx.table().columns
		const sources = [...new Set(indices)]
			.sort((a, b) => a - b)
			.map((i) => edits.rows[i])
			.filter((r): r is Row => !!r)
		let snaps: Row[]
		try {
			snaps = await Promise.all(
				sources.map((row) => cloneRowValues({ row, columns, fetchChunk })),
			)
		} catch (err) {
			this.#ctx.report(err)
			return
		}
		if (snaps.length === 0) return
		edits.noteEdit()
		for (let i = snaps.length - 1; i >= 0; i--) {
			const snap = snaps[i]
			if (snap) this.#insertTop(snap, false)
		}
	}

	/** Drop an unsaved insert/clone from local state (no server DELETE). */
	#removeLocal(index: number) {
		const map = (i: number) => (i === index ? null : i > index ? i - 1 : i)
		this.#ctx.edits.removeLocal(index)
		this.#ctx.editor.remap(map)
		this.#ctx.sel.remap(map)
	}

	deleteRows(indices: number[]) {
		const unique = [...new Set(indices)].sort((a, b) => b - a)
		if (unique.length === 0) return
		this.#ctx.edits.noteEdit()
		for (const i of unique) this.#deleteRow(i, true)
	}

	undeleteRows(indices: number[]) {
		const { edits } = this.#ctx
		if (!indices.some(edits.isDeleted)) return
		edits.noteEdit()
		edits.undelete(indices)
	}

	#deleteRow(index: number, quiet = false) {
		const { edits, editor, sel } = this.#ctx
		if (this.#ctx.readOnly()) {
			if (!quiet) this.#ctx.flash('Connection is read-only')
			return
		}
		this.#commitEdit()
		if (!edits.rows[index] || edits.isDeleted(index)) return
		// Unsaved insert/clone: remove locally. Staging a DELETE has no PK and
		// blocks Save (dirtyCount treats new+deleted as a no-op).
		if (edits.isNew(index)) {
			this.#removeLocal(index)
			return
		}
		const hasPk = this.#ctx
			.table()
			.columns.some((c) => c.pk || c.name === 'id' || c.name === '_id' || c.name === 'rowid')
		if (!hasPk) {
			if (!quiet) showStatus('Cannot delete: table has no primary key', 'warning')
			return
		}
		edits.markDeleted(index)
		if (editor.editing?.row === index) editor.drop()
		if (sel.cell?.row === index) sel.setCell(null)
	}

	/** ⌘Delete: mark the action rows for delete, or undo the mark when they all have it. */
	toggleDelete(): boolean {
		const targets = this.actionRows()
		if (targets.length === 0) return false
		const live = targets.filter((i) => !this.#ctx.edits.isDeleted(i))
		if (live.length) this.deleteRows(live)
		else this.undeleteRows(targets)
		return true
	}

	copyFocusedCell(e?: ClipboardEvent) {
		const cell = this.#ctx.sel.cell
		if (!cell || this.#ctx.editor.editing) return
		copyTableCell(this.#ctx.copy, cell, e)
	}

	/** ⌘C and the copy event: a cell block, several rows, or the focused cell. */
	copy(e?: ClipboardEvent) {
		const { sel } = this.#ctx
		if (sel.kind === 'cells' && sel.cells.size > 1) {
			e?.preventDefault()
			copyTableSelection(this.#ctx.copy, sel, 'copy', this.actionRows())
		} else if (sel.kind === 'rows' && sel.rows.size > 1) {
			e?.preventDefault()
			copyTableRows(this.#ctx.copy, sel.rowList(), 'copy')
		} else {
			this.copyFocusedCell(e)
		}
	}

	paste(text: string) {
		const { edits, editor, sel } = this.#ctx
		if (!this.#ctx.writable()) return
		const list = pasteCellEdits({
			text,
			targets: sel.actionCells().filter((c) => !edits.isDeleted(c.row)),
			focused: sel.cell,
			columns: this.#ctx.visibleColumns(),
			rowCount: edits.rows.length,
			isDeleted: edits.isDeleted,
			toValue: (row, col, raw) => editor.draftToValue(row, col, raw),
		})
		if (list.length === 0) return
		edits.noteEdit()
		edits.setCells(list)
	}

	async #pasteFromClipboard() {
		if (!this.#ctx.writable()) return
		this.#commitEdit()
		try {
			this.paste(await navigator.clipboard.readText())
		} catch {
			showToast('danger', 'Could not read clipboard')
		}
	}

	rowMenu(primary: number): ContextMenuItem[] {
		const { sel, edits } = this.#ctx
		const targets = sel.rows.has(primary) && sel.rows.size > 1 ? sel.rowList() : [primary]
		return tableRowMenu({ readOnly: this.#ctx.readOnly(), targets, isDeleted: edits.isDeleted })
	}

	cellMenu(row: number): ContextMenuItem[] {
		const { sel, edits } = this.#ctx
		const action = sel.actionCells()
		return tableCellMenu({
			readOnly: this.#ctx.readOnly(),
			pending: edits.isDeleted(row),
			targets: this.targetRows(row),
			isDeleted: edits.isDeleted,
			cellCount: action.length,
			allNull: action.length > 0 && action.every((c) => edits.rows[c.row]?.[c.col] == null),
		})
	}

	#runRows(kind: 'clone' | 'delete' | 'undelete' | 'details', row: number) {
		const targets = this.targetRows(row)
		if (kind === 'clone') void this.duplicate(targets)
		else if (kind === 'delete') this.deleteRows(targets.filter((i) => !this.#ctx.edits.isDeleted(i)))
		else if (kind === 'undelete') this.undeleteRows(targets)
		else {
			this.#ctx.sel.focusRow(row)
			this.#ctx.inspect(row)
		}
	}

	pickRowMenu(row: number, id: string) {
		const action = tableRowPick(id)
		if (!action) return
		if (action.kind === 'copy-rows') copyTableRows(this.#ctx.copy, this.targetRows(row), action.format)
		else if (
			action.kind === 'clone' ||
			action.kind === 'delete' ||
			action.kind === 'undelete' ||
			action.kind === 'details'
		) {
			this.#runRows(action.kind, row)
		}
	}

	pickCellMenu(row: number, col: string, id: string) {
		const { sel, editor } = this.#ctx
		sel.focus(row, col)
		const action = tableCellPick(id)
		if (!action) return
		switch (action.kind) {
			case 'edit-modal':
				return editor.openModal(row, col)
			case 'set-null':
				return this.setCellsNull(sel.actionCells())
			case 'copy-cells':
				if (sel.actionCells().length > 1) {
					copyTableSelection(this.#ctx.copy, sel, 'copy', this.actionRows())
				} else {
					this.copyFocusedCell()
				}
				return
			case 'paste':
				return void this.#pasteFromClipboard()
			case 'copy-rows':
				return copyTableRows(this.#ctx.copy, this.targetRows(row), action.format)
			case 'copy-column':
				return void copyCells(col, 'Column name copied', [])
			default:
				return this.#runRows(action.kind, row)
		}
	}
}
