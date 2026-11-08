/**
 * Grid selection as plain data. Both grids use it, so clicks, Shift ranges,
 * ⌘/Ctrl toggles, arrows and Escape act the same in each.
 *
 * Rows are absolute indices. A column is whatever key the grid uses: a column
 * name in the table tab, a column index in query results. Ranges and steps
 * follow the visible column `order` the caller passes, never the data order,
 * so a hidden column is never selected, pasted into, or copied.
 *
 * Every function returns a new state; nothing is mutated.
 */

/** A column key: a name or an index. */
export type Col = string | number

export type SelectionKind = 'none' | 'rows' | 'cells'

export type CellRef<C extends Col> = { row: number; col: C }

export type GridSelectionState<C extends Col> = {
	kind: SelectionKind
	/** Focused cell (keyboard target, copy of one cell). */
	cell: CellRef<C> | null
	/** Focused row: the last row clicked or reached, in either mode. */
	row: number | null
	/** Full-row selection (row-number clicks). */
	rows: ReadonlySet<number>
	/** Shift-range start in row mode. */
	rowAnchor: number | null
	/** Cell selection, keyed by `cellKey`. */
	cells: ReadonlyMap<string, CellRef<C>>
	/** Shift-range start in cell mode. */
	cellAnchor: CellRef<C> | null
}

/** Shift extends, ⌘/Ctrl toggles. */
export type PointerMods = { shift: boolean; mod: boolean }

export type RowBounds = { first: number; count: number }

const NO_ROWS: ReadonlySet<number> = new Set()

export function emptySelection<C extends Col>(): GridSelectionState<C> {
	return {
		kind: 'none',
		cell: null,
		row: null,
		rows: NO_ROWS,
		rowAnchor: null,
		cells: new Map(),
		cellAnchor: null,
	}
}

export function cellKey(row: number, col: string | number): string {
	return `${row}::${col}`
}

export function pointerMods(
	e?: { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean } | null,
): PointerMods {
	return { shift: !!e?.shiftKey, mod: !!(e && (e.metaKey || e.ctrlKey)) }
}

/** Visible columns between two, inclusive. An unknown end counts as the first column. */
export function orderRange<C>(order: readonly C[], from: C, to: C): C[] {
	const a = Math.max(0, order.indexOf(from))
	const b = Math.max(0, order.indexOf(to))
	return order.slice(Math.min(a, b), Math.max(a, b) + 1)
}

/** The visible column `delta` steps from `current`, clamped to the ends. */
export function orderStep<C>(
	order: readonly C[],
	current: C | null | undefined,
	delta: number,
): C | undefined {
	if (order.length === 0) return undefined
	const index = current == null ? 0 : Math.max(0, order.indexOf(current))
	return order[Math.max(0, Math.min(order.length - 1, index + delta))]
}

function rowMode<C extends Col>(s: GridSelectionState<C>): GridSelectionState<C> {
	return { ...s, kind: 'rows', cell: null, cells: new Map(), cellAnchor: null }
}

function cellMode<C extends Col>(s: GridSelectionState<C>): GridSelectionState<C> {
	return { ...s, kind: 'cells', rows: NO_ROWS, rowAnchor: null }
}

export function exclusiveRow<C extends Col>(s: GridSelectionState<C>, row: number): GridSelectionState<C> {
	return { ...rowMode(s), rows: new Set([row]), rowAnchor: row, row }
}

export function toggleRow<C extends Col>(s: GridSelectionState<C>, row: number): GridSelectionState<C> {
	const rows = new Set(s.rows)
	if (rows.has(row)) rows.delete(row)
	else rows.add(row)
	if (rows.size === 0) rows.add(row)
	return { ...rowMode(s), rows, rowAnchor: row, row }
}

/** Rows from the anchor (or the focused row) to `to`. The anchor stays. */
export function rowRange<C extends Col>(s: GridSelectionState<C>, to: number): GridSelectionState<C> {
	const from = s.rowAnchor ?? s.row ?? to
	const rows = new Set<number>()
	for (let i = Math.min(from, to); i <= Math.max(from, to); i++) rows.add(i)
	return { ...rowMode(s), rows, row: to }
}

export function rowPointer<C extends Col>(
	s: GridSelectionState<C>,
	row: number,
	mods: PointerMods,
): GridSelectionState<C> {
	if (mods.shift) return rowRange(s, row)
	if (mods.mod) return toggleRow(s, row)
	return exclusiveRow(s, row)
}

export function exclusiveCell<C extends Col>(
	s: GridSelectionState<C>,
	row: number,
	col: C,
): GridSelectionState<C> {
	const cell = { row, col }
	return {
		...cellMode(s),
		cell,
		cellAnchor: cell,
		cells: new Map([[cellKey(row, col), cell]]),
		row,
	}
}

export function toggleCell<C extends Col>(
	s: GridSelectionState<C>,
	row: number,
	col: C,
): GridSelectionState<C> {
	const cell = { row, col }
	const key = cellKey(row, col)
	const cells = new Map(s.cells)
	if (cells.has(key)) cells.delete(key)
	else cells.set(key, cell)
	if (cells.size === 0) cells.set(key, cell)
	return { ...cellMode(s), cells, cell, cellAnchor: cell, row }
}

/** The rectangle from the anchor (or focused cell) to the target, in visible order. */
export function cellRect<C extends Col>(
	s: GridSelectionState<C>,
	row: number,
	col: C,
	order: readonly C[],
): GridSelectionState<C> {
	const from = s.cellAnchor ?? s.cell ?? { row, col }
	const cols = orderRange(order, from.col, col)
	const cells = new Map<string, CellRef<C>>()
	for (let r = Math.min(from.row, row); r <= Math.max(from.row, row); r++) {
		for (const c of cols) cells.set(cellKey(r, c), { row: r, col: c })
	}
	return { ...cellMode(s), cells, cell: { row, col }, row }
}

export function cellPointer<C extends Col>(
	s: GridSelectionState<C>,
	row: number,
	col: C,
	mods: PointerMods,
	order: readonly C[],
): GridSelectionState<C> {
	if (mods.shift) return cellRect(s, row, col, order)
	if (mods.mod) return toggleCell(s, row, col)
	return exclusiveCell(s, row, col)
}

/** Select every row given. `focusFirst` also focuses the first one when nothing is focused. */
export function allRows<C extends Col>(
	s: GridSelectionState<C>,
	rows: readonly number[],
	focusFirst = false,
): GridSelectionState<C> {
	const next = { ...rowMode(s), rows: new Set(rows) }
	const first = rows[0]
	if (first === undefined) return next
	return {
		...next,
		rowAnchor: s.rowAnchor ?? first,
		row: focusFirst ? (s.row ?? first) : s.row,
	}
}

/** Focus a cell (and its row) without changing what is selected. */
export function focusCell<C extends Col>(s: GridSelectionState<C>, row: number, col: C): GridSelectionState<C> {
	return { ...s, cell: { row, col }, row }
}

/**
 * Escape: a multi-selection shrinks to its focused cell or row. Null when
 * there is nothing to shrink, so the grid decides what clearing means.
 */
export function collapse<C extends Col>(s: GridSelectionState<C>): GridSelectionState<C> | null {
	if (s.kind === 'cells' && s.cells.size > 1 && s.cell) {
		return exclusiveCell(s, s.cell.row, s.cell.col)
	}
	if (s.kind === 'rows' && s.rows.size > 1 && s.row != null) {
		return exclusiveRow(s, s.row)
	}
	return null
}

/** Drop the focused cell and the cell selection; the focused row stays. */
export function clearCells<C extends Col>(s: GridSelectionState<C>): GridSelectionState<C> {
	return { ...s, kind: 'none', cell: null, cells: new Map(), cellAnchor: null }
}

/**
 * Move every row index through `map` (rows inserted or removed above).
 * A row mapped to null is gone: its focus and selection go with it, and an
 * anchor on it falls back to the new focus.
 */
export function remapRows<C extends Col>(
	s: GridSelectionState<C>,
	map: (row: number) => number | null,
): GridSelectionState<C> {
	const moveCell = (c: CellRef<C> | null): CellRef<C> | null => {
		if (!c) return null
		const row = map(c.row)
		return row == null ? null : { row, col: c.col }
	}
	const cell = moveCell(s.cell)
	const row = s.row == null ? null : map(s.row)
	const rows = new Set<number>()
	for (const r of s.rows) {
		const next = map(r)
		if (next != null) rows.add(next)
	}
	const cells = new Map<string, CellRef<C>>()
	for (const c of s.cells.values()) {
		const next = moveCell(c)
		if (next) cells.set(cellKey(next.row, next.col), next)
	}
	const rowAnchor = s.rowAnchor == null ? null : (map(s.rowAnchor) ?? row)
	const cellAnchor = s.cellAnchor ? (moveCell(s.cellAnchor) ?? cell) : null
	return { ...s, cell, row, rows, rowAnchor, cells, cellAnchor }
}

export function isRowSelected<C extends Col>(s: GridSelectionState<C>, row: number): boolean {
	return s.kind === 'rows' && s.rows.has(row)
}

export function isCellSelected<C extends Col>(s: GridSelectionState<C>, row: number, col: C): boolean {
	return s.kind === 'cells' && s.cells.has(cellKey(row, col))
}

export function selectedRowList<C extends Col>(s: GridSelectionState<C>): number[] {
	return [...s.rows].sort((a, b) => a - b)
}

/** Rows that hold a selected cell, ascending. */
export function rowsInCells<C extends Col>(s: GridSelectionState<C>): number[] {
	return [...new Set([...s.cells.values()].map((c) => c.row))].sort((a, b) => a - b)
}

/** The cells an action applies to: selected cells, selected rows × visible columns, or the focused cell. */
export function actionCells<C extends Col>(s: GridSelectionState<C>, order: readonly C[]): CellRef<C>[] {
	if (s.kind === 'cells' && s.cells.size > 0) {
		return [...s.cells.values()].filter((c) => order.includes(c.col))
	}
	if (s.kind === 'rows' && s.rows.size > 0) {
		return selectedRowList(s).flatMap((row) => order.map((col) => ({ row, col })))
	}
	return s.cell ? [{ row: s.cell.row, col: s.cell.col }] : []
}

/**
 * The bounding block of a cell selection, in row and visible-column order.
 * `has` tells which block positions are selected (the rest copy as blanks).
 */
export function selectedBlock<C extends Col>(
	s: GridSelectionState<C>,
	order: readonly C[],
): { rows: number[]; cols: C[]; count: number; has: (row: number, col: C) => boolean } | null {
	if (s.kind !== 'cells' || s.cells.size === 0) return null
	const picked = actionCells(s, order)
	if (picked.length === 0) return null
	const wanted = new Set(picked.map((c) => c.col))
	return {
		rows: [...new Set(picked.map((c) => c.row))].sort((a, b) => a - b),
		cols: order.filter((c) => wanted.has(c)),
		count: picked.length,
		has: (row, col) => s.cells.has(cellKey(row, col)),
	}
}

/** Values under the selection, for the status-bar stats. */
export function selectionValues<C extends Col>(
	s: GridSelectionState<C>,
	cellValue: (row: number, col: C) => unknown,
	rowValues: (row: number) => readonly unknown[] | null,
): unknown[] {
	if (s.kind === 'cells') return [...s.cells.values()].map((c) => cellValue(c.row, c.col))
	if (s.kind === 'rows') return [...s.rows].flatMap((row) => rowValues(row) ?? [])
	return s.cell ? [cellValue(s.cell.row, s.cell.col)] : []
}

/**
 * Arrow / Tab step. Row mode stays in rows unless the step is sideways;
 * Shift extends the range in either mode. Null when the grid is empty.
 */
export function moveSelection<C extends Col>(
	s: GridSelectionState<C>,
	step: { dRow: number; dCol: number; extend: boolean },
	order: readonly C[],
	bounds: RowBounds,
): { state: GridSelectionState<C>; row: number; col: C } | null {
	const col = orderStep(order, s.cell?.col, step.dCol)
	if (col === undefined || bounds.count === 0) return null
	const from = s.cell?.row ?? s.row ?? s.rowAnchor ?? bounds.first
	let index = from - bounds.first
	if (index < 0 || index >= bounds.count) index = 0
	index = Math.max(0, Math.min(bounds.count - 1, index + step.dRow))
	const row = bounds.first + index
	// The target row is focused first, so a row range with no anchor starts there.
	const base = { ...s, row }
	let state: GridSelectionState<C>
	if (s.kind === 'rows' || !s.cell) {
		if (step.dCol !== 0 && !step.extend) state = exclusiveCell(base, row, col)
		else if (!step.extend) state = exclusiveRow(base, row)
		else state = rowRange(base, row)
	} else if (!step.extend) {
		state = exclusiveCell(base, row, col)
	} else {
		state = cellRect(base, row, col, order)
	}
	return { state, row, col }
}
