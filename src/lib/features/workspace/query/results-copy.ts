/**
 * Copy from the query results grid: one cell, a cell block, or whole rows,
 * in the shared "Copy as" formats. Results have no table, so INSERT and
 * UPDATE name it `results`.
 *
 * Large values load in full first when the result carries their key
 * (`loadLarge`); the rest copy as previews and the message says how many.
 */
import type { QueryResult } from '#core/contracts'
import {
	cellClipboardText,
	copyCells,
	copyCellsFull,
	isSqlCopyFormat,
	sqlCopyText,
	textCopyText,
	type CopyFormat,
	type GridSelection,
	type IdentStyle,
	type LargeLoader,
} from '#features/workspace/data-grid'
import type { CellsCopyFormat } from './results-menus'

export type ResultSource = {
	columns: QueryResult['columns']
	/** One row's cells by absolute index, or null outside the page. */
	rowCells: (absRow: number) => unknown[] | null
	identStyle: IdentStyle
	/** Loads a large value in full (query tab session, through its transaction). */
	loadLarge?: LargeLoader | null
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

function jsonFlags(src: ResultSource, cols: readonly number[]): boolean[] {
	return cols.map((i) => (src.columns[i]?.dataType ?? '').toLowerCase().includes('json'))
}

function allColumns(src: ResultSource): number[] {
	return src.columns.map((_, i) => i)
}

function copyResultCell(
	src: ResultSource,
	absRow: number,
	col: number,
	format: CellsCopyFormat,
) {
	const value = src.rowCells(absRow)?.[col]
	const cells = [[value]]
	const json = jsonFlags(src, [col])
	if (format === 'json') {
		void copyCellsFull(
			(full) => `${JSON.stringify(full[0]?.[0] === undefined ? null : full[0][0], null, 2)}\n`,
			'Copied as JSON',
			cells,
			src.loadLarge,
			json,
		)
	} else if (format === 'tsv') {
		const column = { name: src.columns[col]?.name ?? 'value' }
		void copyCellsFull(
			(full) => textCopyText('copy', [column], full),
			'Copied as TSV',
			cells,
			src.loadLarge,
		)
	} else {
		void copyCellsFull(
			(full) => cellClipboardText(full[0]?.[0]),
			'Cell copied',
			cells,
			src.loadLarge,
		)
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
	const json = jsonFlags(src, block.cols)
	if (format === 'json') {
		void copyCellsFull(
			(full) => textCopyText('json', cols, full),
			`Copied ${n} cells as JSON`,
			cells,
			src.loadLarge,
			json,
		)
	} else {
		const message = format === 'tsv' ? `Copied ${n} cells as TSV` : `${n} cells copied`
		void copyCellsFull((full) => textCopyText('copy', cols, full), message, cells, src.loadLarge)
	}
}

export function copyResultRows(src: ResultSource, indices: number[], format: CopyFormat) {
	const cells = indices.map(src.rowCells).filter((r): r is unknown[] => !!r)
	const n = cells.length
	if (n === 0) return
	const label = FORMAT_LABEL[format]
	const json = jsonFlags(src, allColumns(src))
	if (format === 'copy') {
		const message = n === 1 ? 'Row copied' : `${n} rows copied`
		void copyCellsFull(
			(full) => textCopyText('copy', exportColumns(src), full),
			message,
			cells,
			src.loadLarge,
		)
	} else if (isSqlCopyFormat(format)) {
		const cols = src.columns.map((c) => ({ name: c.name, dataType: c.dataType ?? '' }))
		void copyCellsFull(
			(full) => sqlCopyText(format, { style: src.identStyle, table: 'results' }, cols, full),
			`Copied as ${label}`,
			cells,
			src.loadLarge,
		)
	} else {
		const message = n === 1 ? `Copied as ${label}` : `Copied ${n} rows as ${label}`
		void copyCellsFull(
			(full) => textCopyText(format, exportColumns(src), full),
			message,
			cells,
			src.loadLarge,
			format === 'json' ? json : [],
		)
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
