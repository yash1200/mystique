/**
 * Reactive grid selection over `selection-model`. One instance per grid; the
 * grid passes its visible column order (frozen first, hidden left out), and
 * every range, step, copy and paste goes through that order.
 */
import {
	actionCells,
	allRows,
	cellPointer,
	clearCells,
	collapse,
	emptySelection,
	exclusiveCell,
	exclusiveRow,
	focusCell,
	isCellSelected,
	isRowSelected,
	moveSelection,
	pointerMods,
	remapRows,
	rowPointer,
	rowsInCells,
	selectedBlock,
	selectedRowList,
	selectionValues,
	type CellRef,
	type Col,
	type GridSelectionState,
	type RowBounds,
} from './selection-model'

type PointerEventLike = { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean }

export class GridSelection<C extends Col> {
	state = $state.raw<GridSelectionState<C>>(emptySelection())
	readonly #order: () => readonly C[]

	constructor(order: () => readonly C[]) {
		this.#order = order
	}

	get kind() {
		return this.state.kind
	}
	get cell() {
		return this.state.cell
	}
	/** Focused row, in either mode. */
	get row() {
		return this.state.row
	}
	get rows() {
		return this.state.rows
	}
	get cells() {
		return this.state.cells
	}
	/** Visible column order. */
	get order(): readonly C[] {
		return this.#order()
	}

	isRowSelected = (row: number): boolean => isRowSelected(this.state, row)
	isCellSelected = (row: number, col: C): boolean => isCellSelected(this.state, row, col)

	rowList(): number[] {
		return selectedRowList(this.state)
	}
	rowsInCells(): number[] {
		return rowsInCells(this.state)
	}
	actionCells(): CellRef<C>[] {
		return actionCells(this.state, this.order)
	}
	block() {
		return selectedBlock(this.state, this.order)
	}
	values(
		cellValue: (row: number, col: C) => unknown,
		rowValues: (row: number) => readonly unknown[] | null,
	): unknown[] {
		return selectionValues(this.state, cellValue, rowValues)
	}

	/** Row-number click: plain, Shift range, or ⌘/Ctrl toggle. */
	pointRow(row: number, e?: PointerEventLike | null) {
		this.state = rowPointer(this.state, row, pointerMods(e))
	}
	/** Cell click: plain, Shift rectangle, or ⌘/Ctrl toggle. */
	pointCell(row: number, col: C, e?: PointerEventLike | null) {
		this.state = cellPointer(this.state, row, col, pointerMods(e), this.order)
	}
	selectRow(row: number) {
		this.state = exclusiveRow(this.state, row)
	}
	selectCell(row: number, col: C) {
		this.state = exclusiveCell(this.state, row, col)
	}
	selectAll(rows: readonly number[], focusFirst = false) {
		this.state = allRows(this.state, rows, focusFirst)
	}
	/** Focus a cell without changing what is selected. */
	focus(row: number, col: C) {
		this.state = focusCell(this.state, row, col)
	}
	focusRow(row: number | null) {
		this.state = { ...this.state, row }
	}
	/** Set or drop the focused cell only; the focused row and the selection stay. */
	setCell(cell: CellRef<C> | null) {
		this.state = { ...this.state, cell }
	}
	/** Escape: shrink a multi-selection. False when there was nothing to shrink. */
	collapse(): boolean {
		const next = collapse(this.state)
		if (!next) return false
		this.state = next
		return true
	}
	clearCells() {
		this.state = clearCells(this.state)
	}
	/** Select nothing. `keep` leaves the focused row and/or cell where they are. */
	clear(keep: { row?: boolean; cell?: boolean } = {}) {
		this.state = {
			...emptySelection(),
			row: keep.row ? this.state.row : null,
			cell: keep.cell ? this.state.cell : null,
		}
	}
	/** Arrow / Tab. Returns the cell moved to, or null on an empty grid. */
	move(step: { dRow: number; dCol: number; extend: boolean }, bounds: RowBounds) {
		const moved = moveSelection(this.state, step, this.order, bounds)
		if (!moved) return null
		this.state = moved.state
		return { row: moved.row, col: moved.col }
	}
	/** Rows inserted or removed above: move every index through `map`. */
	remap(map: (row: number) => number | null) {
		this.state = remapRows(this.state, map)
	}
}
