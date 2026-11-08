/**
 * Table-grid column widths. All widths here are unscaled CSS px (16px root);
 * the grid multiplies by the UI scale when it draws.
 *
 * Automatic width: header text plus a sample of cell text, clamped. A width
 * the user drags or double-clicks is stored per table and wins over it.
 */
import { cellDisplay } from './virtual'

/** Smallest automatic width; also the drag floor. */
export const COL_W_MIN = 64
/** Automatic width cap, so one long value does not push the rest away. */
export const COL_W_AUTO_MAX = 320
/** Double-click fit cap. */
export const COL_W_FIT_MAX = 800
/** Drag cap. */
export const COL_W_MAX = 2000
/** Width when nothing is known yet (the old fixed `min-w-32`). */
export const COL_W_DEFAULT = 128

/** 12px monospace cell text. */
const CELL_CHAR_PX = 7.25
/** `px-2.5` on both sides plus a little slack for the ellipsis. */
const CELL_PAD_PX = 24
/** 11px header text (name, PK, type). */
const HEAD_CHAR_PX = 6.6
/** Padding, sort button, gaps. */
const HEAD_PAD_PX = 52
const HEAD_MAX = 400
/** FK jump button. */
const REF_BUTTON_PX = 24

export type WidthColumn = {
	name: string
	type?: string | null
	pk?: boolean
	ref?: unknown
}

/** Up to 100 row indexes: the head, the tail, and a spread between. */
export function sampleRowIndexes(count: number, limit = 100): number[] {
	if (count <= limit) return Array.from({ length: count }, (_, i) => i)
	const head = Math.floor(limit * 0.4)
	const tail = Math.floor(limit * 0.2)
	const middle = limit - head - tail
	const out: number[] = []
	for (let i = 0; i < head; i++) out.push(i)
	for (let i = 1; i <= middle; i++) {
		out.push(head + Math.floor(((count - head - tail) * i) / (middle + 1)))
	}
	for (let i = count - tail; i < count; i++) out.push(i)
	return out
}

export function headerWidth(column: WidthColumn): number {
	const chars =
		column.name.length + (column.type ? column.type.length + 1 : 0) + (column.pk ? 3 : 0)
	return Math.min(HEAD_MAX, Math.ceil(chars * HEAD_CHAR_PX) + HEAD_PAD_PX)
}

/** Header and sampled values, clamped to `[COL_W_MIN, max]`. The header always fits up to its cap. */
export function contentWidth(
	column: WidthColumn,
	values: Iterable<unknown>,
	max = COL_W_AUTO_MAX,
): number {
	let longest = 0
	for (const value of values) {
		const length = cellDisplay(value).length
		if (length > longest) longest = length
	}
	const extra = column.ref ? REF_BUTTON_PX : 0
	const cells = Math.ceil(longest * CELL_CHAR_PX) + CELL_PAD_PX + extra
	const head = headerWidth(column)
	return Math.round(Math.max(COL_W_MIN, head, Math.min(max, cells)))
}

/** Automatic widths for every column from a row sample. */
export function autoColumnWidths(
	columns: readonly WidthColumn[],
	rows: readonly Record<string, unknown>[],
): Record<string, number> {
	const sample = sampleRowIndexes(rows.length)
	const out: Record<string, number> = {}
	for (const column of columns) {
		out[column.name] = contentWidth(
			column,
			sample.map((i) => rows[i]?.[column.name]),
		)
	}
	return out
}

/** Double-click fit: every loaded row, with the wider cap. */
export function fitColumnWidth(
	column: WidthColumn,
	rows: readonly Record<string, unknown>[],
): number {
	return contentWidth(
		column,
		rows.map((row) => row[column.name]),
		COL_W_FIT_MAX,
	)
}

/** Width after dragging the header edge by `deltaPx` screen px at `scale`. */
export function dragWidth(startWidth: number, deltaPx: number, scale = 1): number {
	const next = startWidth + deltaPx / (scale > 0 ? scale : 1)
	return Math.round(Math.min(COL_W_MAX, Math.max(COL_W_MIN, next)))
}

/** The width a column draws at: user width, then automatic, then the default. */
export function columnWidth(
	name: string,
	stored: Readonly<Record<string, number>> | undefined,
	auto: Readonly<Record<string, number>>,
): number {
	return stored?.[name] ?? auto[name] ?? COL_W_DEFAULT
}

/** `offsets[i]` is where column `i` starts; the last entry is the total. */
export function columnOffsets(widths: readonly number[]): number[] {
	const out = [0]
	let x = 0
	for (const width of widths) {
		x += width
		out.push(x)
	}
	return out
}
