/** A grid cell whose full text stayed on the server. */

export interface LargeCellKey {
	column: string
	value: unknown
}

export interface LargeCellLocator {
	database?: string | null
	schema?: string | null
	table: string
	column: string
	keys: LargeCellKey[]
}

export interface LargeCell {
	$large: true
	preview: string
	totalChars: number
	locator?: LargeCellLocator | null
}

export interface LargeCellPage {
	text: string
	offset: number
	nextOffset?: number | null
	totalChars: number
}

export const PARTIAL_COPY_MESSAGE =
	'Copied a preview. Open the cell to load the full value.'

export const UNKEYED_LARGE_CELL =
	'This value is larger than the preview. It cannot be loaded without a primary key.'

export function isLargeCell(value: unknown): value is LargeCell {
	if (!value || typeof value !== 'object') return false
	const cell = value as Record<string, unknown>
	return cell.$large === true && typeof cell.preview === 'string'
}

export function largeCellText(value: unknown): string {
	return isLargeCell(value) ? value.preview : ''
}

export function largeCellHint(value: LargeCell): string {
	if (value.locator && value.locator.keys.length > 0) {
		return 'Large value. Open the cell to load the full text.'
	}
	return UNKEYED_LARGE_CELL
}

export function cellsIncludeLarge(rows: unknown[][]): boolean {
	for (const row of rows) {
		for (const cell of row) {
			if (isLargeCell(cell)) return true
		}
	}
	return false
}
