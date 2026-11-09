/**
 * Copy from the table grid: one cell, the selected cell block, or whole rows
 * in the shared "Copy as" formats. INSERT / UPDATE name the real table.
 */
import type { TableMeta } from '#core/contracts'
import { showStatus, showToast } from '#features/toast'
import {
	cellText,
	copyCells,
	copyCellsFull,
	identStyleFor,
	isLargeCell,
	isSqlCopyFormat,
	largeCellText,
	objectsToRows,
	PARTIAL_COPY_MESSAGE,
	sqlCopyText,
	textCopyText,
	type CellRef,
	type CopyFormat,
	type GridSelection,
	type LargeLoader,
	type TextCopyFormat,
} from '#features/workspace/data-grid'

export type TableCopySource = {
	table: TableMeta
	rows: readonly Record<string, unknown>[]
	engine: string
	sqlDialect: string
	/** Loads large values in full before a copy. */
	loadLarge?: LargeLoader | null
}

function jsonColumns(names: readonly string[], table: TableMeta): boolean[] {
	return names.map((name) => {
		const type = table.columns.find((c) => c.name === name)?.type ?? ''
		return type.toLowerCase().includes('json')
	})
}

const FORMAT_LABEL: Record<CopyFormat, string> = {
	copy: '',
	csv: 'CSV',
	markdown: 'Markdown',
	json: 'JSON',
	insert: 'INSERT',
	update: 'UPDATE',
	in: 'IN list',
	where: 'WHERE',
}

/** Rows by index, every column, in the given format. */
export function copyTableRows(src: TableCopySource, indices: readonly number[], format: CopyFormat) {
	const { table } = src
	const columns = table.columns.map((c) => ({ name: c.name }))
	const objects = indices
		.map((i) => src.rows[i])
		.filter((r): r is Record<string, unknown> => !!r)
	const cells = objectsToRows(columns, objects)
	const count = objects.length
	if (count === 0) return
	let build: (cells: unknown[][]) => string
	if (isSqlCopyFormat(format)) {
		const cols = table.columns.map((c) => ({ name: c.name, dataType: c.type, pk: !!c.pk }))
		const target = {
			style: identStyleFor(src.engine, src.sqlDialect),
			table: table.name,
			database: table.database,
			schema: table.schema,
		}
		build = (full) => sqlCopyText(format, target, cols, full)
	} else {
		build = (full) => textCopyText(format, columns, full)
	}
	const message =
		format === 'copy'
			? count === 1
				? 'Row copied'
				: `${count} rows copied`
			: `Copied ${count === 1 ? 'row' : `${count} rows`} as ${FORMAT_LABEL[format]}`
	const json = jsonColumns(columns.map((c) => c.name), table)
	void copyCellsFull(build, message, cells, src.loadLarge, json)
}

/**
 * A multi-cell selection as a block (holes copy blank). Anything else copies
 * `fallbackRows` as rows.
 */
export function copyTableSelection(
	src: TableCopySource,
	sel: GridSelection<string>,
	format: TextCopyFormat,
	fallbackRows: readonly number[],
) {
	const block = sel.kind === 'cells' && sel.cells.size > 1 ? sel.block() : null
	if (!block) {
		copyTableRows(src, fallbackRows, format)
		return
	}
	const columns = block.cols.map((name) => ({ name }))
	const cells = block.rows.map((r) =>
		block.cols.map((name) => (block.has(r, name) ? (src.rows[r]?.[name] ?? '') : '')),
	)
	const n = block.count === 1 ? 'cell' : `${block.count} cells`
	const message =
		format === 'copy' ? `${block.count} cells copied` : `Copied ${n} as ${FORMAT_LABEL[format]}`
	const json = jsonColumns(block.cols, src.table)
	void copyCellsFull((full) => textCopyText(format, columns, full), message, cells, src.loadLarge, json)
}

/**
 * The focused cell. NULL copies as empty text (paste-friendly). A large cell
 * with a key loads in full and copies that; one without copies its preview
 * and says so. In a `copy` event the text goes straight into its clipboard
 * data (a large value cannot load in time, so it is written afterwards).
 */
export function copyTableCell(src: TableCopySource, cell: CellRef, e?: ClipboardEvent) {
	const v = src.rows[cell.row]?.[cell.col]
	const partial = isLargeCell(v)
	if (partial && src.loadLarge && v.locator && v.locator.keys.length > 0) {
		e?.preventDefault()
		void copyCellsFull((full) => cellText(full[0]?.[0]), 'Cell copied', [[v]], src.loadLarge)
		return
	}
	const text = v === null || v === undefined ? '' : partial ? largeCellText(v) : cellText(v)
	if (e?.clipboardData) {
		e.preventDefault()
		e.clipboardData.setData('text/plain', text)
		const message = partial ? `Cell copied. ${PARTIAL_COPY_MESSAGE}` : 'Cell copied'
		if (partial) showToast('warning', message)
		else showStatus(message)
		return
	}
	void copyCells(text, 'Cell copied', [[v]])
}
