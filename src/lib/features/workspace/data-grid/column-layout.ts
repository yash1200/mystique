/** Per-table column freeze, hide, order, and width. Stored outside the user database. */

export type GridColumnLayout = {
	order: string[]
	hidden: string[]
	frozen: string[]
	/** Widths the user set, by column name, in unscaled px. Others are automatic. */
	widths?: Record<string, number>
}

export const EMPTY_COLUMN_LAYOUT: GridColumnLayout = {
	order: [],
	hidden: [],
	frozen: [],
	widths: {},
}

/** IPC shape of `GridColumnLayoutRecord` (mystique-core). */
export type StoredColumnLayout = GridColumnLayout & {
	connectionId: string
	tableId: string
}

export function layoutToStored(
	connectionId: string,
	tableId: string,
	layout: GridColumnLayout,
): StoredColumnLayout {
	return {
		connectionId,
		tableId,
		order: [...layout.order],
		hidden: [...layout.hidden],
		frozen: [...layout.frozen],
		widths: { ...(layout.widths ?? {}) },
	}
}

export function storedToLayout(row: Partial<StoredColumnLayout>): GridColumnLayout {
	return {
		order: row.order ?? [],
		hidden: row.hidden ?? [],
		frozen: row.frozen ?? [],
		widths: row.widths ?? {},
	}
}

/** Set a user width, or clear it with `null` so the column is automatic again. */
export function withColumnWidth(
	layout: GridColumnLayout,
	name: string,
	width: number | null,
): GridColumnLayout {
	const widths = { ...(layout.widths ?? {}) }
	if (width == null) delete widths[name]
	else widths[name] = Math.round(width)
	return { ...layout, widths }
}

export function layoutColumns<T extends { name: string }>(
	columns: readonly T[],
	layout: GridColumnLayout | null | undefined,
): { frozen: T[]; scroll: T[] } {
	const byName = new Map(columns.map((column) => [column.name, column]))
	const hidden = new Set(layout?.hidden ?? [])
	const seen = new Set<string>()
	const names: string[] = []
	for (const name of layout?.order ?? []) {
		if (!byName.has(name) || seen.has(name)) continue
		seen.add(name)
		names.push(name)
	}
	for (const column of columns) {
		if (seen.has(column.name)) continue
		seen.add(column.name)
		names.push(column.name)
	}
	const frozenNames = new Set(layout?.frozen ?? [])
	const frozen: T[] = []
	const scroll: T[] = []
	for (const name of names) {
		if (hidden.has(name)) continue
		const column = byName.get(name)
		if (!column) continue
		if (frozenNames.has(name)) frozen.push(column)
		else scroll.push(column)
	}
	return { frozen, scroll }
}

export function moveColumn(
	layout: GridColumnLayout,
	names: readonly string[],
	name: string,
	direction: -1 | 1,
): GridColumnLayout {
	const order = layout.order.length > 0 ? [...layout.order] : [...names]
	for (const column of names) {
		if (!order.includes(column)) order.push(column)
	}
	const index = order.indexOf(name)
	const next = index + direction
	if (index < 0 || next < 0 || next >= order.length) return { ...layout, order }
	const swap = order[next]
	if (swap === undefined) return { ...layout, order }
	order[index] = swap
	order[next] = name
	return { ...layout, order }
}

export function toggleHidden(layout: GridColumnLayout, name: string): GridColumnLayout {
	const hidden = new Set(layout.hidden)
	if (hidden.has(name)) hidden.delete(name)
	else hidden.add(name)
	const frozen = layout.frozen.filter((column) => column !== name || !hidden.has(name))
	return { ...layout, hidden: [...hidden], frozen }
}

export function toggleFrozen(layout: GridColumnLayout, name: string): GridColumnLayout {
	const frozen = new Set(layout.frozen)
	if (frozen.has(name)) frozen.delete(name)
	else frozen.add(name)
	const hidden = layout.hidden.filter((column) => column !== name)
	return { ...layout, hidden, frozen: [...frozen] }
}
