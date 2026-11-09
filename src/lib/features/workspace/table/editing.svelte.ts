/**
 * Editing a cell of one table tab: in place (the cell input or select) or in
 * the value modal. A committed draft is parsed back into a value the way the
 * column's editor expects and written through `TableEdits`.
 */
import type { ColumnDef } from '#core/contracts'
import {
	cellText,
	isLargeCell,
	parseDraft,
	selectDraftToValue,
	usesSelectEditor,
	valueToEditorText,
	valueToSelectDraft,
	type CellRef,
	type GridSelection,
	type LargeCellLocator,
} from '#features/workspace/data-grid'
import type { TableEdits } from './edits.svelte'

export type CellModalState = {
	row: number
	col: string
	type: string
	text: string
	locator: LargeCellLocator | null
	unfetchable: boolean
}

const MARKED_FOR_DELETE = 'Row is marked for delete — undo delete to edit'

export class CellEditor {
	editing = $state<CellRef | null>(null)
	draft = $state('')
	inputEl = $state<HTMLInputElement | HTMLSelectElement | null>(null)
	modal = $state<CellModalState | null>(null)
	readonly #edits: TableEdits
	readonly #sel: GridSelection<string>
	readonly #column: (name: string) => ColumnDef | undefined
	readonly #writable: () => boolean
	readonly #flash: (message: string) => void
	readonly #focusGrid: () => void
	readonly #move: (dRow: number, dCol: number) => void

	constructor(opts: {
		edits: TableEdits
		sel: GridSelection<string>
		column: (name: string) => ColumnDef | undefined
		/** False (after telling the user) on a read-only connection. */
		writable: () => boolean
		flash: (message: string) => void
		focusGrid: () => void
		/** Enter / Tab after a commit move the selection. */
		move: (dRow: number, dCol: number) => void
	}) {
		this.#edits = opts.edits
		this.#sel = opts.sel
		this.#column = opts.column
		this.#writable = opts.writable
		this.#flash = opts.flash
		this.#focusGrid = opts.focusGrid
		this.#move = opts.move
		$effect(() => {
			const el = this.inputEl
			if (!this.editing || !el) return
			el.focus()
			if (el instanceof HTMLInputElement) el.select()
		})
	}

	/** A draft as the value it stands for: select editors map back to their option, the rest parse. */
	draftToValue(row: number, col: string, rawText: string): unknown {
		const prev = this.#edits.rows[row]?.[col]
		const def = this.#column(col)
		return def && usesSelectEditor(def, prev)
			? selectDraftToValue(rawText, def, prev)
			: parseDraft(rawText, prev)
	}

	/** Parse and write one cell. `track` records an undo step first. */
	apply(row: number, col: string, rawText: string, track = true) {
		if (!this.#writable()) return
		if (track) this.#edits.noteEdit()
		this.#edits.setCell(row, col, this.draftToValue(row, col, rawText))
	}

	/** Edit in place. `seedDraft` (type-to-edit) replaces the cell text. */
	start(row: number, col: string, e?: MouseEvent, seedDraft?: string) {
		e?.stopPropagation()
		if (!this.#writable()) return
		if (this.#edits.isDeleted(row)) {
			this.#flash(MARKED_FOR_DELETE)
			return
		}
		const value = this.#edits.rows[row]?.[col]
		if (isLargeCell(value)) {
			if (seedDraft !== undefined) {
				this.#flash('Open the cell to load the full value before editing.')
				return
			}
			this.openModal(row, col)
			return
		}
		this.#sel.focus(row, col)
		const editing = this.editing
		if (editing?.row === row && editing.col === col && seedDraft === undefined) return
		if (editing) this.commit()
		const def = this.#column(col)
		this.editing = { row, col }
		if (seedDraft !== undefined) this.draft = seedDraft
		else if (def && usesSelectEditor(def, value)) this.draft = valueToSelectDraft(value)
		else this.draft = value === null || value === undefined ? '' : cellText(value)
	}

	commit() {
		const editing = this.editing
		if (!editing) return
		this.apply(editing.row, editing.col, this.draft)
		this.#sel.setCell({ row: editing.row, col: editing.col })
		this.editing = null
		this.draft = ''
		this.#focusGrid()
	}

	cancel() {
		const editing = this.editing
		if (!editing) return
		this.#sel.setCell({ row: editing.row, col: editing.col })
		this.editing = null
		this.draft = ''
		this.#focusGrid()
	}

	/** Leave edit mode without writing (the row is gone or reloaded). */
	drop() {
		this.editing = null
		this.draft = ''
	}

	/** Rows inserted or removed above: follow the edited cell, or stop editing it. */
	remap(map: (row: number) => number | null) {
		const editing = this.editing
		if (!editing) return
		const row = map(editing.row)
		if (row == null) this.drop()
		else this.editing = { row, col: editing.col }
	}

	/** The value modal; `seed` carries an in-place draft over. */
	openModal(row: number, col: string, seed?: string) {
		if (this.#edits.isDeleted(row)) {
			this.#flash(MARKED_FOR_DELETE)
			return
		}
		if (this.editing) this.drop()
		const value = this.#edits.rows[row]?.[col]
		const large = isLargeCell(value) ? value : null
		this.modal = {
			row,
			col,
			type: this.#column(col)?.type ?? '',
			text: seed !== undefined ? seed : valueToEditorText(value),
			locator: large?.locator ?? null,
			unfetchable: !!large && !(large.locator && large.locator.keys.length > 0),
		}
		this.#sel.focus(row, col)
	}

	applyModal(text: string) {
		const modal = this.modal
		if (!modal) return
		this.apply(modal.row, modal.col, text)
		this.#sel.focus(modal.row, modal.col)
		this.modal = null
		this.#focusGrid()
	}

	closeModal() {
		const modal = this.modal
		if (!modal) return
		this.#sel.setCell({ row: modal.row, col: modal.col })
		this.modal = null
		this.#focusGrid()
	}

	/** Keys inside the cell input: Enter commits and moves down, Tab across, Esc cancels. */
	onInputKeydown = (e: KeyboardEvent) => {
		if (e.key === 'Enter' && e.shiftKey) {
			e.preventDefault()
			e.stopPropagation()
			const editing = this.editing
			if (editing) this.openModal(editing.row, editing.col, this.draft)
			return
		}
		if (e.key === 'Enter') {
			e.preventDefault()
			e.stopPropagation()
			this.commit()
			// Excel-like: Enter moves down after commit
			this.#move(1, 0)
		} else if (e.key === 'Escape') {
			e.preventDefault()
			e.stopPropagation()
			this.cancel()
		} else if (e.key === 'Tab') {
			e.preventDefault()
			e.stopPropagation()
			this.commit()
			this.#move(0, e.shiftKey ? -1 : 1)
		}
	}
}
