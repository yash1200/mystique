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

/** For a query result whose source table key is not in the row. */
export const QUERY_UNKEYED_LARGE_CELL =
	'This value is larger than the preview. It loads only when the result includes its table’s key columns and the column comes straight from the table.'

/** Loads one chunk of a large value (table grid, or a query tab). */
export type LargeLoader = (
	locator: LargeCellLocator,
	offset: number,
) => Promise<LargeCellPage>

/**
 * Most characters copy-as or export loads in total. One value is capped on
 * the backend (64 MB); this bounds a block of them.
 */
export const FULL_VALUES_MAX_CHARS = 128 * 1024 * 1024

export function largeCellFetchable(value: LargeCell): boolean {
	return !!value.locator && value.locator.keys.length > 0
}

/** Every chunk of one value. The chunks come from one read on the backend. */
export async function loadLargeText(
	locator: LargeCellLocator,
	load: LargeLoader,
): Promise<string> {
	const parts: string[] = []
	let offset = 0
	for (;;) {
		const page = await load(locator, offset)
		parts.push(page.text)
		if (page.nextOffset == null) return parts.join('')
		if (page.nextOffset <= offset) throw new Error('The large value stopped loading.')
		offset = page.nextOffset
	}
}

export type ResolvedCells = {
	cells: unknown[][]
	/** Large cells left as previews: no key, or nothing to load them with. */
	previews: number
}

/**
 * Replace large-cell previews with their full text before copy or export.
 * `json[c]` parses column `c`'s full text back to JSON. Throws when the
 * values together pass `FULL_VALUES_MAX_CHARS`, or when one fails to load.
 */
export async function resolveLargeCells(
	cells: unknown[][],
	load: LargeLoader | null | undefined,
	json: readonly boolean[] = [],
): Promise<ResolvedCells> {
	let previews = 0
	let total = 0
	let out: unknown[][] | null = null
	for (let r = 0; r < cells.length; r++) {
		const row = cells[r]
		for (let c = 0; c < row.length; c++) {
			const value = row[c]
			if (!isLargeCell(value)) continue
			const locator = value.locator
			if (!load || !locator || locator.keys.length === 0) {
				previews++
				continue
			}
			total += value.totalChars
			if (total > FULL_VALUES_MAX_CHARS) {
				throw new Error(
					`The selected values add up to more than ${FULL_VALUES_MAX_CHARS / (1024 * 1024)} MB. Export the table to a file instead.`,
				)
			}
			const text = await loadLargeText(locator, load)
			out ??= cells.map((cells) => cells.slice())
			out[r][c] = json[c] ? parseJsonText(text) : text
		}
	}
	return { cells: out ?? cells, previews }
}

function parseJsonText(text: string): unknown {
	try {
		return JSON.parse(text)
	} catch {
		return text
	}
}

export function previewsNote(previews: number): string {
	return previews === 1
		? '1 large value is only a preview: its row has no key to load it by.'
		: `${previews} large values are only previews: their rows have no key to load them by.`
}
