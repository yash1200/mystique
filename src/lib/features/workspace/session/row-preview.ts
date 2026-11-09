/**
 * Text for the "Preview affected rows" panel in the write dialog. Plain TS
 * so tests can read it. Cells are rendered as text, never as HTML.
 */
import type { WriteRowPreview } from '#core/contracts'

const VERB: Record<string, string> = {
	update: 'updated',
	delete: 'deleted',
	insert: 'inserted',
}

function rows(n: number): string {
	return `${n.toLocaleString()} row${n === 1 ? '' : 's'}`
}

/** One line above the grid: how many rows, or why there is nothing to show. */
export function previewSummary(preview: WriteRowPreview): string {
	if (!preview.available) {
		return `Preview unavailable. ${preview.reason ?? ''}`.trim()
	}
	if (preview.error) return `Preview failed: ${preview.error}`
	const verb = VERB[preview.action ?? ''] ?? 'changed'
	const shown = preview.rows.length
	if (preview.total != null) {
		if (preview.total === 0) return `0 rows would be ${verb}. Check the WHERE clause.`
		const head = `${rows(preview.total)} would be ${verb}`
		return preview.total > shown ? `${head}. Showing the first ${shown}.` : `${head}.`
	}
	const head = `${rows(shown)}${preview.truncated ? ' or more' : ''} would be ${verb}`
	return preview.note ? `${head}. ${preview.note}` : `${head}.`
}

/** The preview found nothing to change: the dialog says so loudly. */
export function previewIsEmpty(preview: WriteRowPreview): boolean {
	return preview.available && !preview.error && preview.total === 0
}

/** A cell as plain text. Large values show their preview. */
export function previewCell(value: unknown): string {
	if (value === null || value === undefined) return 'NULL'
	if (typeof value === 'string') return value
	if (typeof value === 'number' || typeof value === 'boolean') return String(value)
	if (typeof value === 'object') {
		const large = value as { $large?: unknown; preview?: unknown }
		if (large.$large === true && typeof large.preview === 'string') {
			return `${large.preview}…`
		}
		try {
			return JSON.stringify(value)
		} catch {
			return String(value)
		}
	}
	return String(value)
}

/** UPDATE's `"<column> (new)"` columns get the highlight. */
export function isNewColumn(preview: WriteRowPreview, name: string): boolean {
	return preview.newColumns.includes(name)
}
