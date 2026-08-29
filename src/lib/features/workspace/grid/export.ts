/**
 * Serialize result rows as CSV / TSV / JSON / Markdown / Excel / PDF.
 * File dialogs and byte writing live in `$core/util/files`.
 */
import type { TableFileFormat } from '$core/contracts'
import {
	safeFileBase,
	saveBytesWithPrompt,
	shortStamp,
	type SaveResult,
} from '$core/util/files'
import { jsPDF } from 'jspdf'

export type { SaveResult, TableFileFormat }

export type TableExportColumn = { name: string }

function stripSqlNoise(sql: string): string {
	return sql
		.replace(/\/\*[\s\S]*?\*\//g, ' ')
		.replace(/--[^\n]*/g, ' ')
		.replace(/\s+/g, ' ')
		.trim()
}

function lastSqlStatement(sql: string): string {
	const parts = stripSqlNoise(sql)
		.split(';')
		.map((s) => s.trim())
		.filter(Boolean)
	return parts[parts.length - 1] ?? stripSqlNoise(sql)
}

/** Strip SQL identifier quotes/brackets and join schema.table. */
function cleanSqlIdent(raw: string): string {
	return raw
		.split('.')
		.map((p) =>
			p
				.trim()
				.replace(/^["`\[]+/, '')
				.replace(/["`\]]+$/, '')
				.trim(),
		)
		.filter(Boolean)
		.join('.')
}

const SQL_IDENT = '(?:"[^"]+"|`[^`]+`|\\[[^\\]]+\\]|[a-zA-Z_][\\w$]*)'

const SQL_QUALIFIED = `${SQL_IDENT}(?:\\s*\\.\\s*${SQL_IDENT})?`

/** Best-effort primary relation name from a SQL statement. */
export function extractSqlRelationName(queryText: string): string | null {
	const stmt = lastSqlStatement(queryText)
	if (!stmt) return null

	const patterns: RegExp[] = [
		new RegExp(`\\bFROM\\s+(${SQL_QUALIFIED})`, 'i'),
		new RegExp(`\\bUPDATE\\s+(${SQL_QUALIFIED})`, 'i'),
		new RegExp(`\\bINTO\\s+(?:TABLE\\s+)?(${SQL_QUALIFIED})`, 'i'),
		new RegExp(`\\bTABLE\\s+(${SQL_QUALIFIED})`, 'i'),
		new RegExp(
			`\\b(?:DESCRIBE|DESC|EXPLAIN)\\s+(?:TABLE\\s+)?(${SQL_QUALIFIED})`,
			'i',
		),
		new RegExp(`\\bSHOW\\s+CREATE\\s+TABLE\\s+(${SQL_QUALIFIED})`, 'i'),
	]

	for (const re of patterns) {
		const m = stmt.match(re)
		if (m?.[1]) {
			const name = cleanSqlIdent(m[1])
			// Skip common subquery openers / CTE noise
			if (name && !/^\($/.test(name) && name.toLowerCase() !== 'select') {
				return name
			}
		}
	}
	return null
}

/** Best-effort collection name from MQL / mongo shell-ish text. */
export function extractMqlCollectionName(
	queryText: string,
	collectionHint?: string | null,
): string | null {
	const hint = collectionHint?.trim()
	if (hint) return hint

	const text = queryText.trim()
	if (!text) return null

	// db.users.find(...) / db["users"].aggregate(...)
	const dbDot = text.match(
		/\bdb\s*(?:\.|\[\s*["'])([a-zA-Z_][\w$]*|[^"'\]]+)(?:["']\s*\])?\s*\./,
	)
	if (dbDot?.[1]) return dbDot[1].trim()

	// collection: "users" in JSON body
	const collField = text.match(/["']collection["']\s*:\s*["']([^"']+)["']/)
	if (collField?.[1]) return collField[1].trim()

	return null
}

/**
 * Build a descriptive download base name from the active query context.
 * Examples: `Analytics-public.users-20260729-1430`, `local-orders-20260729-1430`
 */
export function exportFileBaseFromQuery(opts: {
	queryText: string
	language?: string | null
	connectionName?: string | null
	collection?: string | null
	database?: string | null
}): string {
	const lang = (opts.language ?? 'sql').toLowerCase()
	const queryText = opts.queryText ?? ''

	let subject: string | null = null
	if (lang === 'mql') {
		subject = extractMqlCollectionName(queryText, opts.collection)
		if (!subject && opts.database?.trim()) {
			subject = opts.database.trim()
		}
	} else {
		subject = extractSqlRelationName(queryText)
	}

	if (!subject) {
		// Fallback: first meaningful token(s) of the query
		const cleaned = stripSqlNoise(queryText)
			.replace(/[^\w\s.\-]/g, ' ')
			.trim()
		const words = cleaned.split(/\s+/).filter(Boolean).slice(0, 4)
		subject = words.length > 0 ? words.join('-') : 'query-results'
	}

	const parts: string[] = []
	const conn = opts.connectionName?.trim()
	if (conn) parts.push(conn)
	if (
		lang === 'mql' &&
		opts.database?.trim() &&
		subject !== opts.database.trim()
	) {
		// db.collection style when both known
		if (!subject.includes('.')) {
			parts.push(`${opts.database.trim()}.${subject}`)
		} else {
			parts.push(subject)
		}
	} else {
		parts.push(subject)
	}
	parts.push(shortStamp())

	return safeFileBase(parts.join('-'))
}

function cellToString(v: unknown): string {
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
export function csvEscape(value: string): string {
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

export async function exportRowsAsCsv(
	columns: TableExportColumn[],
	rows: unknown[][],
	fileBase = 'query-results',
): Promise<SaveResult> {
	const csv = buildCsv(columns, rows)
	const bytes = new TextEncoder().encode(csv)
	return saveBytesWithPrompt(
		bytes,
		`${safeFileBase(fileBase)}.csv`,
		[{ name: 'CSV', extensions: ['csv'] }],
		'text/csv;charset=utf-8',
	)
}

/**
 * Embed JSON object/array cell values as JSON, not as escaped strings.
 * Already-parsed objects are left as-is. Plain text is unchanged.
 */
function reviveJsonCell(value: unknown): unknown {
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

/** Array of row objects (column name → value). */
export function buildJsonObjects(
	columns: TableExportColumn[],
	rows: unknown[][],
): Record<string, unknown>[] {
	return rows.map((row) => {
		const obj: Record<string, unknown> = {}
		for (let i = 0; i < columns.length; i++) {
			const raw = row[i] ?? null
			obj[columns[i]?.name ?? `col_${i}`] = reviveJsonCell(raw)
		}
		return obj
	})
}

export function buildJson(
	columns: TableExportColumn[],
	rows: unknown[][],
	pretty = true,
): string {
	const data = buildJsonObjects(columns, rows)
	return pretty
		? `${JSON.stringify(data, null, 2)}\n`
		: `${JSON.stringify(data)}\n`
}

/** Write UTF-8 text into `folder/fileName`. Browser fallback downloads the file. */
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

export async function exportRowsAsJson(
	columns: TableExportColumn[],
	rows: unknown[][],
	fileBase = 'query-results',
): Promise<SaveResult> {
	const json = buildJson(columns, rows, true)
	const bytes = new TextEncoder().encode(json)
	return saveBytesWithPrompt(
		bytes,
		`${safeFileBase(fileBase)}.json`,
		[{ name: 'JSON', extensions: ['json'] }],
		'application/json;charset=utf-8',
	)
}

/**
 * Excel-compatible SpreadsheetML (.xls). Opens in Excel / LibreOffice without
 * an xlsx dependency.
 */
export function buildExcelXml(
	columns: TableExportColumn[],
	rows: unknown[][],
	sheetName = 'Results',
): string {
	const escapeXml = (s: string) =>
		s
			.replace(/&/g, '&amp;')
			.replace(/</g, '&lt;')
			.replace(/>/g, '&gt;')
			.replace(/"/g, '&quot;')

	const cellXml = (v: unknown): string => {
		if (v === null || v === undefined) {
			return `<Cell><Data ss:Type="String"></Data></Cell>`
		}
		if (typeof v === 'number' && Number.isFinite(v)) {
			return `<Cell><Data ss:Type="Number">${v}</Data></Cell>`
		}
		if (typeof v === 'boolean') {
			return `<Cell><Data ss:Type="Boolean">${v ? 1 : 0}</Data></Cell>`
		}
		const s = cellToString(v)
		// Excel cell limit ~32767
		const clipped = s.length > 32000 ? `${s.slice(0, 32000)}…` : s
		return `<Cell><Data ss:Type="String">${escapeXml(clipped)}</Data></Cell>`
	}

	const header = `<Row>${columns
		.map(
			(c) => `<Cell><Data ss:Type="String">${escapeXml(c.name)}</Data></Cell>`,
		)
		.join('')}</Row>`
	const body = rows
		.map(
			(row) => `<Row>${columns.map((_, i) => cellXml(row[i])).join('')}</Row>`,
		)
		.join('')

	const safeSheet = escapeXml((sheetName || 'Results').slice(0, 31))

	return `<?xml version="1.0" encoding="UTF-8"?>
<?mso-application progid="Excel.Sheet"?>
<Workbook xmlns="urn:schemas-microsoft-com:office:spreadsheet"
 xmlns:o="urn:schemas-microsoft-com:office:office"
 xmlns:x="urn:schemas-microsoft-com:office:excel"
 xmlns:ss="urn:schemas-microsoft-com:office:spreadsheet"
 xmlns:html="http://www.w3.org/TR/REC-html40">
 <Worksheet ss:Name="${safeSheet}">
  <Table>
${header}
${body}
  </Table>
 </Worksheet>
</Workbook>
`
}

export async function exportRowsAsExcel(
	columns: TableExportColumn[],
	rows: unknown[][],
	fileBase = 'query-results',
): Promise<SaveResult> {
	const xml = buildExcelXml(columns, rows)
	const bytes = new TextEncoder().encode(xml)
	return saveBytesWithPrompt(
		bytes,
		`${safeFileBase(fileBase)}.xls`,
		[{ name: 'Excel', extensions: ['xls'] }],
		'application/vnd.ms-excel',
	)
}

/**
 * Simple multi-page landscape PDF table (text). Best for moderate column counts.
 */
export async function exportRowsAsPdf(
	columns: TableExportColumn[],
	rows: unknown[][],
	opts?: { title?: string; fileBase?: string },
): Promise<SaveResult> {
	const title = opts?.title ?? 'Query results'
	const fileBase = opts?.fileBase ?? 'query-results'
	const doc = new jsPDF({
		orientation: 'landscape',
		unit: 'pt',
		format: 'a4',
	})

	const pageW = doc.internal.pageSize.getWidth()
	const pageH = doc.internal.pageSize.getHeight()
	const margin = 28
	const usableW = pageW - margin * 2
	const colCount = Math.max(1, columns.length)
	const colW = usableW / colCount
	const rowH = 14
	const fontSize = colCount > 10 ? 7 : colCount > 6 ? 8 : 9
	const maxCellChars = Math.max(6, Math.floor(colW / (fontSize * 0.48)))

	function trunc(s: string): string {
		if (s.length <= maxCellChars) return s
		return `${s.slice(0, Math.max(1, maxCellChars - 1))}…`
	}

	let y = margin
	doc.setFont('helvetica', 'bold')
	doc.setFontSize(11)
	doc.text(title, margin, y)
	y += 16
	doc.setFont('helvetica', 'normal')
	doc.setFontSize(8)
	doc.setTextColor(100)
	doc.text(
		`${rows.length} row(s) · ${columns.length} column(s) · exported ${new Date().toISOString()}`,
		margin,
		y,
	)
	doc.setTextColor(0)
	y += 14

	function drawHeader() {
		doc.setFont('helvetica', 'bold')
		doc.setFontSize(fontSize)
		doc.setFillColor(240, 242, 245)
		doc.rect(margin, y - rowH + 4, usableW, rowH, 'F')
		columns.forEach((c, i) => {
			const x = margin + i * colW + 2
			doc.text(trunc(c.name), x, y, { maxWidth: colW - 4 })
		})
		y += rowH
		doc.setFont('helvetica', 'normal')
	}

	drawHeader()

	for (let ri = 0; ri < rows.length; ri++) {
		if (y + rowH > pageH - margin) {
			doc.addPage()
			y = margin
			drawHeader()
		}
		if (ri % 2 === 1) {
			doc.setFillColor(248, 249, 251)
			doc.rect(margin, y - rowH + 4, usableW, rowH, 'F')
		}
		const row = rows[ri] ?? []
		columns.forEach((_, i) => {
			const x = margin + i * colW + 2
			doc.text(trunc(cellToString(row[i])), x, y, { maxWidth: colW - 4 })
		})
		y += rowH
	}

	const ab = doc.output('arraybuffer') as ArrayBuffer
	const bytes = new Uint8Array(ab)
	return saveBytesWithPrompt(
		bytes,
		`${safeFileBase(fileBase)}.pdf`,
		[{ name: 'PDF', extensions: ['pdf'] }],
		'application/pdf',
	)
}
