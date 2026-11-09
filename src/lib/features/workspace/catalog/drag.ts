/**
 * Dragging a table out of the explorer (into the agent composer, for one).
 * The payload names the table only; nothing is fetched until it is dropped.
 * The explorer always shows the active connection, the same one the agent
 * pane is bound to.
 */

export const TABLE_DRAG_TYPE = 'application/x-mystique-table'

export type TableDragPayload = {
	database: string
	schema: string
	name: string
}

export function writeTableDrag(
	dataTransfer: DataTransfer,
	payload: TableDragPayload,
): void {
	dataTransfer.setData(TABLE_DRAG_TYPE, JSON.stringify(payload))
	dataTransfer.setData(
		'text/plain',
		payload.schema ? `${payload.schema}.${payload.name}` : payload.name,
	)
	dataTransfer.effectAllowed = 'copy'
}

export function hasTableDrag(dataTransfer: DataTransfer | null): boolean {
	return !!dataTransfer && Array.from(dataTransfer.types).includes(TABLE_DRAG_TYPE)
}

export function readTableDrag(
	dataTransfer: DataTransfer | null,
): TableDragPayload | null {
	const raw = dataTransfer?.getData(TABLE_DRAG_TYPE)
	if (!raw) return null
	try {
		const value = JSON.parse(raw) as Partial<TableDragPayload>
		if (typeof value.name !== 'string' || !value.name) return null
		return {
			database: String(value.database ?? ''),
			schema: String(value.schema ?? ''),
			name: value.name,
		}
	} catch {
		return null
	}
}
