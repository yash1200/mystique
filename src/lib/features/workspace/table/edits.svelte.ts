/**
 * Pending edits of one table tab: the rows as edited, the saved baseline
 * (`originals`), and which cells, inserted rows and deleted rows differ from
 * it. Undo / redo snapshot all of it. Row indices are absolute on the page.
 *
 * The baseline is the grid's bindable `originals` prop (the row inspector
 * reads it to mark changed fields), so it is passed in as an accessor.
 */
import { cellKey, valuesEqual } from '#features/workspace/data-grid'
import type { EditBuffer } from '#features/workspace/session'
import {
	cloneSnapshot,
	pushUndo,
	redoEdit,
	undoEdit,
	type GridEditSnapshot,
} from './edit-history'
import { applyCellBatch, type CellEdit } from './paste-batch'

type Row = Record<string, unknown>

const copyRows = (rows: readonly Row[]): Row[] => rows.map((r) => ({ ...r }))

/** `row::col` keys moved through `map`; a key on a removed row is dropped. */
function remapKeys(keys: ReadonlySet<string>, map: (row: number) => number | null): Set<string> {
	const next = new Set<string>()
	for (const key of keys) {
		const sep = key.indexOf('::')
		if (sep < 0) continue
		const row = Number(key.slice(0, sep))
		const col = key.slice(sep + 2)
		if (!Number.isFinite(row) || !col) continue
		const mapped = map(row)
		if (mapped != null) next.add(cellKey(mapped, col))
	}
	return next
}

function remapSet(set: ReadonlySet<number>, map: (row: number) => number | null): Set<number> {
	const next = new Set<number>()
	for (const i of set) {
		const mapped = map(i)
		if (mapped != null) next.add(mapped)
	}
	return next
}

export class TableEdits {
	/** The page as last loaded from the server (Refresh without a loader returns to it). */
	seed = $state<Row[]>([])
	rows = $state.raw<Row[]>([])
	/** Edited cells (`row::col`). */
	dirtyKeys = $state<Set<string>>(new Set())
	/** Rows added locally, not yet saved. */
	newRows = $state<Set<number>>(new Set())
	/** Saved rows marked for delete, not yet saved. */
	deleted = $state<Set<number>>(new Set())
	undoStack = $state<GridEditSnapshot[]>([])
	redoStack = $state<GridEditSnapshot[]>([])
	#restoring = false
	readonly #columns: () => readonly { name: string }[]
	readonly #baseline: { get: () => Row[]; set: (rows: Row[]) => void }

	constructor(opts: {
		columns: () => readonly { name: string }[]
		baseline: { get: () => Row[]; set: (rows: Row[]) => void }
	}) {
		this.#columns = opts.columns
		this.#baseline = opts.baseline
	}

	/** Last saved rows. */
	get originals(): Row[] {
		return this.#baseline.get()
	}
	set originals(rows: Row[]) {
		this.#baseline.set(rows)
	}

	/** Edited cells + inserted rows + pending deletes. An inserted row marked deleted counts as nothing. */
	dirtyCount = $derived.by(() => {
		let inserts = 0
		for (const i of this.newRows) if (!this.deleted.has(i)) inserts += 1
		let deletes = 0
		for (const i of this.deleted) if (!this.newRows.has(i)) deletes += 1
		return this.dirtyKeys.size + inserts + deletes
	})

	isDirty = (row: number, col: string): boolean => this.dirtyKeys.has(cellKey(row, col))
	isNew = (row: number): boolean => this.newRows.has(row)
	isDeleted = (row: number): boolean => this.deleted.has(row)

	/** Any edit, insert or delete mark at all (even ones that cancel out). */
	hasMarks(): boolean {
		return this.dirtyKeys.size > 0 || this.newRows.size > 0 || this.deleted.size > 0
	}

	/** Saved rows the next save will DELETE. */
	pendingDeletes(): number {
		return [...this.deleted].filter((i) => !this.newRows.has(i)).length
	}

	/** The fields `persistTableEdits` needs. */
	payload() {
		return {
			rows: this.rows,
			originals: this.originals,
			newRowIndices: this.newRows,
			deletedIndices: this.deleted,
			dirtyKeys: this.dirtyKeys,
		}
	}

	/** What the session keeps so a remount restores the edits. */
	buffer(page: number): Omit<EditBuffer, 'tabId'> {
		return {
			baseline: copyRows(this.originals),
			rows: copyRows(this.rows),
			dirtyKeys: [...this.dirtyKeys],
			newRowIndices: [...this.newRows],
			deletedIndices: [...this.deleted],
			page,
		}
	}

	restore(buf: EditBuffer) {
		this.rows = copyRows(buf.rows)
		this.originals = copyRows(buf.baseline)
		this.seed = copyRows(buf.baseline)
		this.dirtyKeys = new Set(buf.dirtyKeys)
		this.newRows = new Set(buf.newRowIndices)
		this.deleted = new Set(buf.deletedIndices)
	}

	/**
	 * Take rows from the table. With `resetBaseline` (load, save, reset) they
	 * become the saved baseline and every mark clears; returns the adopted
	 * rows. Otherwise (an in-place inspector write) the baseline stays and the
	 * changed cells stay dirty; returns null.
	 */
	load(source: readonly Row[], resetBaseline: boolean): Row[] | null {
		const cloned = copyRows(source)
		this.rows = copyRows(cloned)
		if (resetBaseline || this.originals.length === 0) {
			this.seed = copyRows(cloned)
			this.originals = copyRows(cloned)
			this.#clearMarks()
			return cloned
		}
		this.#recomputeDirty(cloned, this.originals)
		return null
	}

	#clearMarks() {
		this.dirtyKeys = new Set()
		this.newRows = new Set()
		this.deleted = new Set()
	}

	#recomputeDirty(rows: readonly Row[], base: readonly Row[]) {
		const next = new Set<string>()
		const columns = this.#columns()
		for (let i = 0; i < rows.length; i++) {
			for (const col of columns) {
				if (!valuesEqual(rows[i]?.[col.name], base[i]?.[col.name])) {
					next.add(cellKey(i, col.name))
				}
			}
		}
		this.dirtyKeys = next
	}

	snapshot(): GridEditSnapshot {
		return {
			rows: copyRows(this.rows),
			originals: copyRows(this.originals),
			dirtyKeys: [...this.dirtyKeys],
			newRows: [...this.newRows],
			deleted: [...this.deleted],
		}
	}

	/** Record an undo step before an edit. */
	noteEdit() {
		if (this.#restoring) return
		this.undoStack = pushUndo(this.undoStack, this.snapshot())
		this.redoStack = []
	}

	clearHistory() {
		this.undoStack = []
		this.redoStack = []
	}

	#apply(snapshot: GridEditSnapshot) {
		this.#restoring = true
		const next = cloneSnapshot(snapshot)
		this.rows = next.rows
		this.originals = next.originals
		this.dirtyKeys = new Set(next.dirtyKeys)
		this.newRows = new Set(next.newRows)
		this.deleted = new Set(next.deleted)
		this.#restoring = false
	}

	/** False when there is nothing to undo. */
	undo(): boolean {
		const step = undoEdit(this.undoStack, this.redoStack, this.snapshot())
		if (!step) return false
		this.undoStack = step.undo
		this.redoStack = step.redo
		this.#apply(step.current)
		return true
	}

	/** False when there is nothing to redo. */
	redo(): boolean {
		const step = redoEdit(this.undoStack, this.redoStack, this.snapshot())
		if (!step) return false
		this.undoStack = step.undo
		this.redoStack = step.redo
		this.#apply(step.current)
		return true
	}

	/** Write one parsed value. The cell is dirty unless it equals the saved value. */
	setCell(row: number, col: string, value: unknown) {
		this.rows = this.rows.map((r, i) => (i === row ? { ...r, [col]: value } : r))
		const key = cellKey(row, col)
		const next = new Set(this.dirtyKeys)
		if (valuesEqual(value, this.originals[row]?.[col])) next.delete(key)
		else next.add(key)
		this.dirtyKeys = next
	}

	/** Write many cells with one row-array assignment (paste). */
	setCells(edits: readonly CellEdit[]) {
		this.dirtyKeys = applyCellBatch(this.rows, this.originals, this.dirtyKeys, edits, (next) => {
			this.rows = next
		})
	}

	/**
	 * Put an unsaved row first. Its baseline equals its values, so only real
	 * edits dirty its cells; `newRows` keeps Save and Reset showing.
	 */
	insertTop(row: Row) {
		const shift = (i: number) => i + 1
		this.newRows = remapSet(this.newRows, shift)
		this.deleted = remapSet(this.deleted, shift)
		this.dirtyKeys = remapKeys(this.dirtyKeys, shift)
		const copy = { ...row }
		this.rows = [copy, ...this.rows]
		this.originals = [{ ...copy }, ...this.originals]
		this.seed = [{ ...copy }, ...this.seed]
		this.newRows = new Set([0, ...this.newRows])
	}

	/** Drop an unsaved insert or clone from the page (no server DELETE). */
	removeLocal(index: number) {
		const map = (i: number) => (i === index ? null : i > index ? i - 1 : i)
		this.rows = this.rows.filter((_, i) => i !== index)
		this.originals = this.originals.filter((_, i) => i !== index)
		this.seed = this.seed.filter((_, i) => i !== index)
		this.newRows = remapSet(this.newRows, map)
		this.deleted = remapSet(this.deleted, map)
		this.dirtyKeys = remapKeys(this.dirtyKeys, map)
	}

	markDeleted(index: number) {
		this.deleted = new Set([...this.deleted, index])
	}

	/** Clear delete marks. False when none of `indices` was marked. */
	undelete(indices: readonly number[]): boolean {
		if (!indices.some((i) => this.deleted.has(i))) return false
		const next = new Set(this.deleted)
		for (const i of indices) next.delete(i)
		this.deleted = next
		return true
	}

	/** Throw away every edit: inserted rows go, the rest return to the baseline. Returns the rows. */
	reset(): Row[] {
		if (this.newRows.size > 0) {
			const keep = this.rows.map((r, i) => ({ r, i })).filter(({ i }) => !this.newRows.has(i))
			this.rows = keep.map(({ r }) => ({ ...r }))
			this.originals = keep.map(({ i }) => ({ ...(this.originals[i] ?? {}) }))
			this.seed = keep.map(({ i }) => ({ ...(this.seed[i] ?? {}) }))
			this.newRows = new Set()
		}
		this.deleted = new Set()
		const restored = copyRows(this.originals)
		this.rows = copyRows(restored)
		this.dirtyKeys = new Set()
		return restored
	}

	/** A save landed and there is no server reload: the edited rows (minus deletes) are the new baseline. */
	adoptSaved(): Row[] {
		const saved = this.rows.filter((_, i) => !this.deleted.has(i)).map((r) => ({ ...r }))
		this.originals = copyRows(saved)
		this.seed = copyRows(saved)
		this.rows = copyRows(saved)
		this.#clearMarks()
		return saved
	}

	/** Refresh without a loader: back to the last loaded page. Returns those rows. */
	revertToSeed(): Row[] {
		const cloned = copyRows(this.seed)
		this.rows = copyRows(cloned)
		this.originals = copyRows(cloned)
		this.#clearMarks()
		return cloned
	}
}
