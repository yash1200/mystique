/** Download or copy a query result as CSV / JSON / Excel. */
import type { QueryResult } from '#core/contracts'
import { errorMessage } from '#core/ipc'
import { copyTextToClipboard } from '#core/util'
import { showToast } from '#features/toast'
import {
	buildCsv,
	buildJson,
	exportFileBaseFromQuery,
	exportRowsAsCsv,
	exportRowsAsExcel,
	exportRowsAsJson,
} from '#features/workspace/grid'
import type { ResultExportAction } from './QueryResultsPanel.svelte'

const DONE: Record<ResultExportAction, string> = {
	'download-csv': 'CSV downloaded',
	'download-json': 'JSON downloaded',
	'download-excel': 'Excel downloaded',
	'copy-csv': 'CSV copied',
	'copy-json': 'JSON copied',
}

/** Does this result have anything to export? */
export function canExportResult(result: QueryResult | null | undefined): boolean {
	return !!result && result.columns.length > 0 && result.rows.length > 0
}

/** Export one result and toast the outcome. */
export async function exportResult(
	action: ResultExportAction,
	result: QueryResult,
	file: Parameters<typeof exportFileBaseFromQuery>[0],
) {
	if (!canExportResult(result)) return
	const cols = result.columns.map((c) => ({ name: c.name }))
	const rows = result.rows
	const fileBase = exportFileBaseFromQuery(file)
	const copy = action === 'copy-csv' || action === 'copy-json'
	try {
		let res
		switch (action) {
			case 'download-csv':
				res = await exportRowsAsCsv(cols, rows, fileBase)
				break
			case 'download-json':
				res = await exportRowsAsJson(cols, rows, fileBase)
				break
			case 'download-excel':
				res = await exportRowsAsExcel(cols, rows, fileBase)
				break
			case 'copy-csv':
				res = await copyTextToClipboard(buildCsv(cols, rows))
				break
			case 'copy-json':
				res = await copyTextToClipboard(buildJson(cols, rows, true))
				break
		}
		if (res.ok) showToast('success', DONE[action])
		else if (!res.cancelled) {
			showToast('danger', res.error || (copy ? 'Copy failed' : 'Export failed'))
		}
	} catch (e) {
		showToast('danger', errorMessage(e))
	}
}
