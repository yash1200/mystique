/**
 * One paste assigns the row array once. Tests spy on `assign`.
 */
import { valuesEqual } from '#features/workspace/data-grid'

export type CellEdit = { row: number; col: string; value: unknown }

export function applyCellBatch(
	rows: readonly Record<string, unknown>[],
	originals: readonly Record<string, unknown>[],
	dirtyKeys: ReadonlySet<string>,
	edits: readonly CellEdit[],
	assign: (next: Record<string, unknown>[]) => void,
): Set<string> {
	if (edits.length === 0) return new Set(dirtyKeys)
	const byRow = new Map<number, CellEdit[]>()
	for (const edit of edits) {
		const list = byRow.get(edit.row)
		if (list) list.push(edit)
		else byRow.set(edit.row, [edit])
	}
	const next = rows.map((row, index) => {
		const editsForRow = byRow.get(index)
		if (!editsForRow) return row
		const copy = { ...row }
		for (const edit of editsForRow) copy[edit.col] = edit.value
		return copy
	})
	assign(next)
	const dirty = new Set(dirtyKeys)
	for (const edit of edits) {
		const key = `${edit.row}::${edit.col}`
		const original = originals[edit.row]?.[edit.col]
		if (valuesEqual(edit.value, original)) dirty.delete(key)
		else dirty.add(key)
	}
	return dirty
}

/**
 * Cell edits for pasted text. One value fills every target cell; a TSV block
 * lands from the top-left target (or the focused cell) across visible
 * columns only, skipping rows marked for delete. Rows are page indices.
 */
export function pasteCellEdits(input: {
	text: string
	/** Selected cells that are not on deleted rows. */
	targets: readonly { row: number; col: string }[]
	focused: { row: number; col: string } | null
	/** Visible column order. */
	columns: readonly string[]
	rowCount: number
	isDeleted: (row: number) => boolean
	toValue: (row: number, col: string, text: string) => unknown
}): CellEdit[] {
	const { columns, targets, toValue } = input
	if (columns.length === 0) return []
	const lines = input.text.replace(/\r\n/g, '\n').replace(/\r/g, '\n').split('\n')
	if (lines.length > 1 && lines[lines.length - 1] === '') lines.pop()
	if (lines.length === 0) return []
	const parsed = lines.map((line) => line.split('\t'))
	const first = parsed[0]
	const edits: CellEdit[] = []

	// One clipboard value fills every selected cell (or the focused cell).
	if (parsed.length === 1 && (first?.length ?? 0) === 1 && targets.length > 0) {
		const value = first?.[0] ?? ''
		for (const { row, col } of targets) edits.push({ row, col, value: toValue(row, col, value) })
		return edits
	}

	const start =
		(targets.length
			? targets.reduce((best, c) => {
					if (c.row < best.row) return c
					if (c.row === best.row && columns.indexOf(c.col) < columns.indexOf(best.col)) {
						return c
					}
					return best
				})
			: null) ?? input.focused
	if (!start) return []
	const startCol = columns.indexOf(start.col)
	if (startCol < 0) return []
	const startRow = start.row >= 0 && start.row < input.rowCount ? start.row : 0
	for (let r = 0; r < parsed.length; r++) {
		const cells = parsed[r]
		const row = startRow + r
		if (!cells || row >= input.rowCount) break
		if (input.isDeleted(row)) continue
		// Visible columns only: a hidden column is skipped, never written.
		columns.slice(startCol, startCol + cells.length).forEach((col, c) => {
			edits.push({ row, col, value: toValue(row, col, cells[c] ?? '') })
		})
	}
	return edits
}
