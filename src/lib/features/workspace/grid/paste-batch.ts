/**
 * One paste assigns the row array once. Tests spy on `assign`.
 */
import { valuesEqual } from './filters'

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
