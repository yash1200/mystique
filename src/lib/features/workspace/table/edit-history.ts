/** Undo stack for pending grid edits. A committed save is not an entry. */

const UNDO_CAP = 100

export type GridEditSnapshot = {
	rows: Record<string, unknown>[]
	originals: Record<string, unknown>[]
	dirtyKeys: string[]
	newRows: number[]
	deleted: number[]
}

export function cloneSnapshot(snapshot: GridEditSnapshot): GridEditSnapshot {
	return {
		rows: snapshot.rows.map((row) => ({ ...row })),
		originals: snapshot.originals.map((row) => ({ ...row })),
		dirtyKeys: [...snapshot.dirtyKeys],
		newRows: [...snapshot.newRows],
		deleted: [...snapshot.deleted],
	}
}

export function pushUndo(
	undo: readonly GridEditSnapshot[],
	snapshot: GridEditSnapshot,
	cap = UNDO_CAP,
): GridEditSnapshot[] {
	const next = [...undo, cloneSnapshot(snapshot)]
	if (next.length <= cap) return next
	return next.slice(next.length - cap)
}

export function undoEdit(
	undo: readonly GridEditSnapshot[],
	redo: readonly GridEditSnapshot[],
	current: GridEditSnapshot,
): { undo: GridEditSnapshot[]; redo: GridEditSnapshot[]; current: GridEditSnapshot } | null {
	const previous = undo.at(-1)
	if (!previous) return null
	return {
		undo: undo.slice(0, -1),
		redo: pushUndo(redo, current),
		current: cloneSnapshot(previous),
	}
}

export function redoEdit(
	undo: readonly GridEditSnapshot[],
	redo: readonly GridEditSnapshot[],
	current: GridEditSnapshot,
): { undo: GridEditSnapshot[]; redo: GridEditSnapshot[]; current: GridEditSnapshot } | null {
	const next = redo.at(-1)
	if (!next) return null
	return {
		undo: pushUndo(undo, current),
		redo: redo.slice(0, -1),
		current: cloneSnapshot(next),
	}
}
