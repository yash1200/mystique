/**
 * Copy rows or a cell block to the clipboard in one of the "Copy as" formats,
 * and tell the user how it went. Both grids use this; each keeps its own
 * wording for the message.
 */
import { copyTextToClipboard } from '#core/util'
import { showStatus, showToast } from '#features/toast'
import {
	buildInList,
	buildInsertSql,
	buildUpdateSql,
	buildWhereSql,
	type CopyColumn,
	type IdentStyle,
} from './copy-as'
import { cellsIncludeLarge, isLargeCell, largeCellText, PARTIAL_COPY_MESSAGE } from './large-cell'
import {
	buildCsv,
	buildJson,
	buildMarkdown,
	buildTsv,
	type TableExportColumn,
} from './text-formats'

export type SqlCopyFormat = 'insert' | 'update' | 'in' | 'where'
export type TextCopyFormat = 'copy' | 'csv' | 'markdown' | 'json'
export type CopyFormat = TextCopyFormat | SqlCopyFormat

export const SQL_COPY_FORMATS: readonly SqlCopyFormat[] = ['insert', 'update', 'in', 'where']

export function isSqlCopyFormat(format: CopyFormat): format is SqlCopyFormat {
	return (SQL_COPY_FORMATS as readonly string[]).includes(format)
}

/** Where INSERT / UPDATE text points. */
export type SqlCopyTarget = {
	style: IdentStyle
	table: string
	database?: string
	schema?: string | null
}

/** SQL text for rows. IN and WHERE need no table. */
export function sqlCopyText(
	format: SqlCopyFormat,
	target: SqlCopyTarget,
	columns: CopyColumn[],
	cells: unknown[][],
): string {
	const { style, table, database, schema } = target
	if (format === 'insert') return buildInsertSql(style, table, columns, cells, database, schema)
	if (format === 'update') return buildUpdateSql(style, table, columns, cells, database, schema)
	if (format === 'in') return buildInList(style, columns, cells)
	return buildWhereSql(style, columns, cells)
}

/** TSV (no header), CSV, Markdown or pretty JSON for a block of cells. */
export function textCopyText(
	format: TextCopyFormat,
	columns: TableExportColumn[],
	cells: unknown[][],
): string {
	if (format === 'csv') return buildCsv(columns, cells)
	if (format === 'markdown') return buildMarkdown(columns, cells)
	if (format === 'json') return buildJson(columns, cells, true)
	return buildTsv(columns, cells)
}

/** One cell as clipboard text: empty for NULL, the loaded preview for a large cell. */
export function cellClipboardText(value: unknown): string {
	if (value === null || value === undefined) return ''
	if (isLargeCell(value)) return largeCellText(value)
	if (typeof value === 'object') {
		try {
			return JSON.stringify(value)
		} catch {
			return String(value)
		}
	}
	return String(value)
}

/** Status line on success (a warning when some cells were partial), toast on failure. */
export function notifyCopy(
	res: { ok: boolean; cancelled?: boolean; error?: string },
	message: string,
	partial = false,
) {
	if (res.ok && partial) showToast('warning', message)
	else if (res.ok) showStatus(message)
	else if (!res.cancelled) showToast('danger', res.error || 'Copy failed')
}

/**
 * Write `text` and report it. When `cells` hold a large value not yet loaded,
 * the message says the copy is partial.
 */
export async function copyCells(text: string, message: string, cells: unknown[][]) {
	const partial = cellsIncludeLarge(cells)
	const res = await copyTextToClipboard(text)
	notifyCopy(res, partial ? `${message}. ${PARTIAL_COPY_MESSAGE}` : message, partial)
}
