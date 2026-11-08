/**
 * Duplicate a grid row. A large cell holds only a preview, so its full value
 * is fetched first. When it cannot be fetched the duplicate is refused rather
 * than inserting the `{"$large":true}` marker.
 */
import { isLargeCell, type LargeCellLocator, type LargeCellPage } from '#features/workspace/data-grid'

export const CLONE_LARGE_REFUSED =
	'Cannot duplicate: a large value in this row cannot be loaded without a primary key.'

export type FetchChunk = (locator: LargeCellLocator, offset: number) => Promise<LargeCellPage>

/** Reads every chunk of one large value. */
export async function fullLargeText(
	locator: LargeCellLocator,
	fetchChunk: FetchChunk,
): Promise<string> {
	const parts: string[] = []
	let offset = 0
	for (;;) {
		const page = await fetchChunk(locator, offset)
		parts.push(page.text)
		if (page.nextOffset == null) return parts.join('')
		if (page.nextOffset <= offset) throw new Error('The large value stopped loading.')
		offset = page.nextOffset
	}
}

/**
 * Row copy for a new insert. Key columns are cleared; large previews are
 * replaced with the full value. Throws `CLONE_LARGE_REFUSED` when a large
 * value has no locator or no way to fetch it.
 */
export async function cloneRowValues(input: {
	row: Record<string, unknown>
	columns: readonly { name: string; type?: string; pk?: boolean }[]
	fetchChunk: FetchChunk | null
}): Promise<Record<string, unknown>> {
	const out: Record<string, unknown> = { ...input.row }
	for (const column of input.columns) {
		if (isCloneKey(column)) out[column.name] = null
	}
	for (const column of input.columns) {
		const value = out[column.name]
		if (!isLargeCell(value)) continue
		const locator = value.locator
		if (!locator || locator.keys.length === 0 || !input.fetchChunk) {
			throw new Error(CLONE_LARGE_REFUSED)
		}
		const text = await fullLargeText(locator, input.fetchChunk)
		out[column.name] = isJsonType(column.type) ? parseJson(text) : text
	}
	return out
}

function isCloneKey(column: { name: string; pk?: boolean }): boolean {
	return (
		!!column.pk ||
		column.name === 'id' ||
		column.name === '_id' ||
		column.name === 'rowid'
	)
}

function isJsonType(type: string | undefined): boolean {
	return !!type && type.toLowerCase().includes('json')
}

function parseJson(text: string): unknown {
	try {
		return JSON.parse(text)
	} catch {
		return text
	}
}
