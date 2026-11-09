/**
 * Copy from the query results grid: one cell, a cell block, or whole rows,
 * in the shared "Copy as" formats. Results have no table, so INSERT and
 * UPDATE name it `results`.
 */
import type { QueryResult } from '#core/contracts'
import {
	cellClipboardText,
	copyCells,
	isLargeCell,
	isSqlCopyFormat,
	largeCellText,
	sqlCopyText,
	textCopyText,
	type CopyFormat,
	type GridSelection,
	type IdentStyle,
} from '#features/workspace/data-grid'
import type { CellsCopyFormat } from './results-menus'

export type ResultSource = {
	columns: QueryResult['columns']
	/** One row's cells by absolute index, or null outside the page. */
	rowCells: (absRow: number) => unknown[] | null
	identStyle: IdentStyle
}

const FORMAT_LABEL: Record<CopyFormat, string> = {
	copy: '',
	csv: 'CSV',
	json: 'JSON',
	markdown: 'Markdown',
	insert: 'INSERT',
	update: 'UPDATE',
	in: 'IN list',
	where: 'WHERE',
}

function exportColumns(src: ResultSource): { name: string }[] {
	return src.columns.map((c) => ({ name: c.name }))
}

function copyResultCell(
	src: ResultSource,
	absRow: number,
	col: number,
	format: CellsCopyFormat,
) {
	const value = src.rowCells(absRow)?.[col]
	const cells = [[value]]
	if (format === 'json') {
		const text = isLargeCell(value)
			? largeCellText(value)
			: `${JSON.stringify(value === undefined ? null : value, null, 2)}\n`
		void copyCells(text, 'Copied as JSON', cells)
	} else if (format === 'tsv') {
		const column = { name: src.columns[col]?.name ?? 'value' }
		void copyCells(textCopyText('copy', [column], cells), 'Copied as TSV', cells)
	} else {
		void copyCells(cellClipboardText(value), 'Cell copied', cells)
	}
}

/** The selected cell block. A single cell copies like `copyResultCell`. */
export function copyResultCells(
	src: ResultSource,
	sel: GridSelection<number>,
	format: CellsCopyFormat,
) {
	const block = sel.block()
	const focused = sel.cell
	if (!block || (block.count === 1 && focused)) {
		if (focused) copyResultCell(src, focused.row, focused.col, format)
		return
	}
	const cols = block.cols.map((i) => ({ name: src.columns[i]?.name ?? `col_${i}` }))
	const cells = block.rows.map((r) =>
		block.cols.map((c) => (block.has(r, c) ? (src.rowCells(r)?.[c] ?? '') : '')),
	)
	const n = block.count
	if (format === 'json') {
		void copyCells(textCopyText('json', cols, cells), `Copied ${n} cells as JSON`, cells)
	} else {
		const message = format === 'tsv' ? `Copied ${n} cells as TSV` : `${n} cells copied`
		void copyCells(textCopyText('copy', cols, cells), message, cells)
	}
}

export function copyResultRows(src: ResultSource, indices: number[], format: CopyFormat) {
	const cells = indices.map(src.rowCells).filter((r): r is unknown[] => !!r)
	const n = cells.length
	if (n === 0) return
	const label = FORMAT_LABEL[format]
	if (format === 'copy') {
		const message = n === 1 ? 'Row copied' : `${n} rows copied`
		void copyCells(textCopyText('copy', exportColumns(src), cells), message, cells)
	} else if (isSqlCopyFormat(format)) {
		const cols = src.columns.map((c) => ({ name: c.name, dataType: c.dataType ?? '' }))
		const text = sqlCopyText(format, { style: src.identStyle, table: 'results' }, cols, cells)
		void copyCells(text, `Copied as ${label}`, cells)
	} else {
		const message = n === 1 ? `Copied as ${label}` : `Copied ${n} rows as ${label}`
		void copyCells(textCopyText(format, exportColumns(src), cells), message, cells)
	}
}

export function copyColumnName(src: ResultSource, col: number) {
	void copyCells(src.columns[col]?.name ?? '', 'Column name copied', [])
}

/** ⌘/Ctrl+C: the cell block, the selected rows, or the focused cell. */
export function copyResultSelection(src: ResultSource, sel: GridSelection<number>) {
	if (sel.kind === 'cells' && sel.cells.size > 1) copyResultCells(src, sel, 'copy')
	else if (sel.kind === 'rows' && sel.rows.size > 0) copyResultRows(src, sel.rowList(), 'copy')
	else if (sel.cell) copyResultCell(src, sel.cell.row, sel.cell.col, 'copy')
}
