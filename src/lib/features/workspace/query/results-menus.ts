/** Context menus of the query results grid (read-only: view and copy). */
import type { ContextMenuItem } from '#core/ui'
import type { CopyFormat } from '#features/workspace/data-grid'

/** Copy formats of the selected cells. */
export type CellsCopyFormat = 'copy' | 'json' | 'tsv'

export type ResultCellPick =
	| { kind: 'view' }
	| { kind: 'cells'; format: CellsCopyFormat }
	| { kind: 'row'; format: CopyFormat }
	| { kind: 'column' }

const ROW_FORMATS: { format: CopyFormat; label: string }[] = [
	{ format: 'csv', label: 'CSV' },
	{ format: 'json', label: 'JSON' },
	{ format: 'markdown', label: 'Markdown' },
	{ format: 'insert', label: 'INSERT' },
	{ format: 'update', label: 'UPDATE' },
]

export function resultCellMenu(many: boolean, copyShortcut: string): ContextMenuItem[] {
	return [
		{ id: 'view', label: 'View in modal', shortcut: '⇧Enter', disabled: many },
		{ id: 'copy', label: 'Copy', shortcut: copyShortcut, separatorBefore: true },
		{ id: 'copy-json', label: 'Copy as JSON' },
		{ id: 'copy-tsv', label: 'Copy as TSV' },
		{ id: 'copy-row', label: 'Copy row', separatorBefore: true },
		...ROW_FORMATS.map(({ format, label }) => ({
			id: `copy-row-${format}`,
			label: `Copy row as ${label}`,
		})),
		{ id: 'copy-row-in', label: 'Copy as IN list' },
		{ id: 'copy-row-where', label: 'Copy as WHERE' },
		{ id: 'copy-column', label: 'Copy column name', separatorBefore: true },
	]
}

export function resultRowMenu(copyShortcut: string): ContextMenuItem[] {
	return [
		{ id: 'copy', label: 'Copy', shortcut: copyShortcut },
		{ id: 'copy-csv', label: 'Copy as CSV' },
		{ id: 'copy-json', label: 'Copy as JSON' },
		{ id: 'copy-markdown', label: 'Copy as Markdown' },
		{ id: 'copy-insert', label: 'Copy as INSERT' },
		{ id: 'copy-update', label: 'Copy as UPDATE' },
		{ id: 'copy-in', label: 'Copy as IN list' },
		{ id: 'copy-where', label: 'Copy as WHERE' },
	]
}

const ROW_PICKS: Record<string, CopyFormat> = {
	copy: 'copy',
	'copy-csv': 'csv',
	'copy-json': 'json',
	'copy-markdown': 'markdown',
	'copy-insert': 'insert',
	'copy-update': 'update',
	'copy-in': 'in',
	'copy-where': 'where',
}

/** Row-menu id → the format to copy the rows in. */
export function resultRowPick(id: string): CopyFormat | null {
	return ROW_PICKS[id] ?? null
}

/** Cell-menu id → what to do. */
export function resultCellPick(id: string): ResultCellPick | null {
	if (id === 'view') return { kind: 'view' }
	if (id === 'copy') return { kind: 'cells', format: 'copy' }
	if (id === 'copy-json') return { kind: 'cells', format: 'json' }
	if (id === 'copy-tsv') return { kind: 'cells', format: 'tsv' }
	if (id === 'copy-column') return { kind: 'column' }
	if (id === 'copy-row') return { kind: 'row', format: 'copy' }
	if (id.startsWith('copy-row-')) {
		const format = resultRowPick(`copy-${id.slice('copy-row-'.length)}`)
		if (format) return { kind: 'row', format }
	}
	return null
}
