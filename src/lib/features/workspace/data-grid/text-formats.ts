/**
 * Rows as text: TSV, CSV, JSON and Markdown. Clipboard copy in both grids,
 * the row inspector, and file export all use these.
 */
import { isLargeCell, largeCellText } from './large-cell'

export type TableExportColumn = { name: string }

/** One cell as plain text. A large cell gives its loaded preview. */
export function cellToString(v: unknown): string {
	if (isLargeCell(v)) return largeCellText(v)
	if (v === null || v === undefined) return ''
	if (typeof v === 'string') return v
	if (typeof v === 'number' || typeof v === 'boolean') return String(v)
	try {
		return JSON.stringify(v)
	} catch {
		return String(v)
	}
}

/** RFC-style CSV cell escape. */
function csvEscape(value: string): string {
	if (/[",\r\n]/.test(value)) {
		return `"${value.replace(/"/g, '""')}"`
	}
	return value
}

/** Build a CSV string from column headers + row-major cells. */
export function buildCsv(
	columns: TableExportColumn[],
	rows: unknown[][],
): string {
	const header = columns.map((c) => csvEscape(c.name)).join(',')
	const body = rows
		.map((row) =>
			columns.map((_, i) => csvEscape(cellToString(row[i]))).join(','),
		)
		.join('\n')
	// BOM helps Excel open UTF-8 correctly
	return `\uFEFF${header}\n${body}\n`
}

/** Object rows (DataGrid) → row-major cells. */
export function objectsToRows(
	columns: TableExportColumn[],
	objects: Record<string, unknown>[],
): unknown[][] {
	return objects.map((obj) => columns.map((c) => obj[c.name]))
}

/**
 * Embed JSON object/array cell values as JSON, not as escaped strings.
 * Already-parsed objects are left as-is. Plain text is unchanged.
 */
function reviveJsonCell(value: unknown): unknown {
	if (isLargeCell(value)) return largeCellText(value)
	if (value == null) return null
	if (typeof value === 'object') return value
	if (typeof value !== 'string') return value
	const t = value.trim()
	if (t.length < 2) return value
	const isObject = t.startsWith('{') && t.endsWith('}')
	const isArray = t.startsWith('[') && t.endsWith(']')
	if (!isObject && !isArray) return value
	try {
		return JSON.parse(t)
	} catch {
		return value
	}
}

function jsonReplacer(_key: string, value: unknown): unknown {
	if (typeof value === 'bigint') return value.toString()
	return value
}

function jsonObject(
	columns: TableExportColumn[],
	valueAt: (index: number, name: string) => unknown,
): Record<string, unknown> {
	const obj: Record<string, unknown> = {}
	for (let i = 0; i < columns.length; i++) {
		const name = columns[i]?.name ?? `col_${i}`
		obj[name] = reviveJsonCell(valueAt(i, name) ?? null)
	}
	return obj
}

function stringifyJson(data: unknown, pretty: boolean, trailingNl: boolean): string {
	try {
		const body = pretty
			? JSON.stringify(data, jsonReplacer, 2)
			: JSON.stringify(data, jsonReplacer)
		return trailingNl ? `${body}\n` : body
	} catch {
		const fallback = Array.isArray(data) ? '[]' : '{}'
		return trailingNl ? `${fallback}\n` : fallback
	}
}

/** One row object (column name → value), same cell rules as export copy. */
function buildJsonObject(
	columns: TableExportColumn[],
	row: Record<string, unknown>,
): Record<string, unknown> {
	return jsonObject(columns, (_i, name) => row[name])
}

/** Array of row objects (column name → value). */
function buildJsonObjects(
	columns: TableExportColumn[],
	rows: unknown[][],
): Record<string, unknown>[] {
	return rows.map((row) => jsonObject(columns, (i) => row[i]))
}

/** Pretty-printed single row. Used by the row inspector view / copy. */
export function buildRowJson(
	columns: TableExportColumn[],
	row: Record<string, unknown>,
	pretty = true,
): string {
	return stringifyJson(buildJsonObject(columns, row), pretty, false)
}

export function buildJson(
	columns: TableExportColumn[],
	rows: unknown[][],
	pretty = true,
): string {
	return stringifyJson(buildJsonObjects(columns, rows), pretty, true)
}

/** Tab-separated values (no header) — paste-friendly single/multi row copy. */
export function buildTsv(
	columns: TableExportColumn[],
	rows: unknown[][],
): string {
	return rows
		.map((row) => columns.map((_, i) => cellToString(row[i])).join('\t'))
		.join('\n')
}

/** GitHub-flavored markdown table. */
export function buildMarkdown(
	columns: TableExportColumn[],
	rows: unknown[][],
): string {
	const esc = (s: string) => s.replace(/\|/g, '\\|').replace(/\r?\n/g, ' ')
	const header = `| ${columns.map((c) => esc(c.name)).join(' | ')} |`
	const sep = `| ${columns.map(() => '---').join(' | ')} |`
	const body = rows
		.map(
			(row) =>
				`| ${columns.map((_, i) => esc(cellToString(row[i]))).join(' | ')} |`,
		)
		.join('\n')
	return `${header}\n${sep}\n${body}\n`
}

