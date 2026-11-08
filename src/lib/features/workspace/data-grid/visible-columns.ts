/**
 * Columns in the order the grid shows them: frozen first, then the rest.
 * Hidden columns are left out. Selection, paste, keys, and copy all use this,
 * so a visible index always means the same column the user sees.
 */
import { layoutColumns, type GridColumnLayout } from './column-layout'

export type VisibleColumns = {
	/** Visible index → data column name. */
	names: string[]
	/** The first `frozen` names stay pinned on the left. */
	frozen: number
}

export function visibleColumns(
	columns: readonly { name: string }[],
	layout: GridColumnLayout | null | undefined,
): VisibleColumns {
	const placed = layoutColumns(columns, layout)
	return {
		names: [...placed.frozen, ...placed.scroll].map((column) => column.name),
		frozen: placed.frozen.length,
	}
}

/** Visible columns between two cells, inclusive. A hidden end clamps to the first column. */
export function columnRange(view: VisibleColumns, from: string, to: string): string[] {
	const a = Math.max(0, view.names.indexOf(from))
	const b = Math.max(0, view.names.indexOf(to))
	return view.names.slice(Math.min(a, b), Math.max(a, b) + 1)
}

/** Up to `count` visible columns from `start`. Empty when `start` is hidden. */
export function columnsFrom(view: VisibleColumns, start: string, count: number): string[] {
	const index = view.names.indexOf(start)
	if (index < 0) return []
	return view.names.slice(index, index + count)
}

/** The visible column `delta` steps from `current`, clamped to the ends. */
export function stepColumn(
	view: VisibleColumns,
	current: string | null | undefined,
	delta: number,
): string | undefined {
	if (view.names.length === 0) return undefined
	const index = current == null ? 0 : Math.max(0, view.names.indexOf(current))
	return view.names[Math.max(0, Math.min(view.names.length - 1, index + delta))]
}

/** Sort selected column names into visible order. Hidden names are dropped. */
export function inVisibleOrder(view: VisibleColumns, names: Iterable<string>): string[] {
	const wanted = new Set(names)
	return view.names.filter((name) => wanted.has(name))
}
