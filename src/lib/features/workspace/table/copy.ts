/**
 * Copy from the table grid: one cell, the selected cell block, or whole rows
 * in the shared "Copy as" formats. INSERT / UPDATE name the real table.
 */
import type { TableMeta } from '#core/contracts'
import { showStatus, showToast } from '#features/toast'
import {
	cellText,
	copyCells,
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
	type TextCopyFormat,
} from '#features/workspace/data-grid'

export type TableCopySource = {
	table: TableMeta
	rows: readonly Record<string, unknown>[]
	engine: string
	sqlDialect: string
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
	let text: string
	if (isSqlCopyFormat(format)) {
		const cols = table.columns.map((c) => ({ name: c.name, dataType: c.type, pk: !!c.pk }))
		const target = {
			style: identStyleFor(src.engine, src.sqlDialect),
			table: table.name,
			database: table.database,
			schema: table.schema,
		}
		text = sqlCopyText(format, target, cols, cells)
	} else {
		text = textCopyText(format, columns, cells)
	}
	const message =
		format === 'copy'
			? count === 1
				? 'Row copied'
				: `${count} rows copied`
			: `Copied ${count === 1 ? 'row' : `${count} rows`} as ${FORMAT_LABEL[format]}`
	void copyCells(text, message, cells)
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
	void copyCells(textCopyText(format, columns, cells), message, cells)
}

/**
 * The focused cell. NULL copies as empty text (paste-friendly); a large cell
 * copies its preview until the modal has loaded the rest. In a `copy` event
 * the text goes straight into its clipboard data.
 */
export function copyTableCell(src: TableCopySource, cell: CellRef, e?: ClipboardEvent) {
	const v = src.rows[cell.row]?.[cell.col]
	const partial = isLargeCell(v)
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
