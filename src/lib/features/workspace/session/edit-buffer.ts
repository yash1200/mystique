/**
 * Per-tab table edits. Lives outside `DataGrid` so a tab remount cannot
 * adopt unsaved rows as the server baseline.
 */

export type GridRow = Record<string, unknown>

export type EditBuffer = {
	tabId: string
	/** Rows last loaded from the server. Local edits do not replace this. */
	baseline: GridRow[]
	rows: GridRow[]
	dirtyKeys: string[]
	newRowIndices: number[]
	deletedIndices: number[]
	page: number
}

const buffers = new Map<string, EditBuffer>()

function copyRows(rows: GridRow[]): GridRow[] {
	return rows.map((row) => ({ ...row }))
}

function copyBuffer(buf: EditBuffer): EditBuffer {
	return {
		tabId: buf.tabId,
		baseline: copyRows(buf.baseline),
		rows: copyRows(buf.rows),
		dirtyKeys: [...buf.dirtyKeys],
		newRowIndices: [...buf.newRowIndices],
		deletedIndices: [...buf.deletedIndices],
		page: buf.page,
	}
}

export function getEditBuffer(tabId: string): EditBuffer | undefined {
	const buf = buffers.get(tabId)
	return buf ? copyBuffer(buf) : undefined
}

export function bufferIsDirty(buf: EditBuffer): boolean {
	const inserts = buf.newRowIndices.filter(
		(index) => !buf.deletedIndices.includes(index),
	).length
	const deletes = buf.deletedIndices.filter(
		(index) => !buf.newRowIndices.includes(index),
	).length
	return buf.dirtyKeys.length + inserts + deletes > 0
}

export function tabEditsAreDirty(tabId: string): boolean {
	const buf = buffers.get(tabId)
	return buf ? bufferIsDirty(buf) : false
}

/** First open, confirmed refresh, or save. Replaces the baseline. */
export function adoptServerRows(
	tabId: string,
	rows: GridRow[],
	page = 0,
): EditBuffer {
	const baseline = copyRows(rows)
	const buf: EditBuffer = {
		tabId,
		baseline,
		rows: copyRows(baseline),
		dirtyKeys: [],
		newRowIndices: [],
		deletedIndices: [],
		page,
	}
	buffers.set(tabId, buf)
	return copyBuffer(buf)
}

/** Remember the grid's current edit state. Baseline stays unless replaced. */
export function rememberEditBuffer(
	tabId: string,
	snap: {
		baseline: GridRow[]
		rows: GridRow[]
		dirtyKeys: string[]
		newRowIndices: number[]
		deletedIndices: number[]
		page: number
	},
): EditBuffer {
	const buf: EditBuffer = {
		tabId,
		baseline: copyRows(snap.baseline),
		rows: copyRows(snap.rows),
		dirtyKeys: [...snap.dirtyKeys],
		newRowIndices: [...snap.newRowIndices],
		deletedIndices: [...snap.deletedIndices],
		page: snap.page,
	}
	buffers.set(tabId, buf)
	return copyBuffer(buf)
}

/**
 * Confirmed refresh. Keeps the current page unless the caller asked to
 * jump back to the first page.
 */
export function confirmServerRefresh(
	tabId: string,
	rows: GridRow[],
	jumpToFirst = false,
): EditBuffer {
	const page = jumpToFirst ? 0 : (buffers.get(tabId)?.page ?? 0)
	return adoptServerRows(tabId, rows, page)
}

/** Dirty reopen focuses the tab. A clean tab may reload. */
export function reopenTableAction(tabId: string): 'focus' | 'load' {
	return tabEditsAreDirty(tabId) ? 'focus' : 'load'
}

/**
 * Remount keeps unsaved edits. A clean empty buffer is not an edit: the grid
 * stored the catalog leaf (no columns, no rows) and then unmounted for the
 * preview spinner. The real page arrives on that same tick, so restoring the
 * leaf would swallow it.
 */
export function shouldRestoreEditBuffer(
	existing: EditBuffer | undefined,
	serverRowCount: number,
): existing is EditBuffer {
	if (!existing) return false
	if (
		!bufferIsDirty(existing) &&
		existing.rows.length === 0 &&
		serverRowCount > 0
	) {
		return false
	}
	return true
}

export function dropEditBuffer(tabId: string) {
	buffers.delete(tabId)
}

export function clearEditBuffers() {
	buffers.clear()
}

/** Values a save would send: baseline is the pre-edit row. */
export function savePayload(buf: EditBuffer): {
	dirty: boolean
	baseline: GridRow[]
	rows: GridRow[]
} {
	return {
		dirty: bufferIsDirty(buf),
		baseline: copyRows(buf.baseline),
		rows: copyRows(buf.rows),
	}
}
