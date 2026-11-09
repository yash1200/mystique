/**
 * Save result rows to a file as CSV / JSON / Excel, named after the query.
 * File dialogs and byte writing live in `#core/util/files.js`.
 */
import {
	safeFileBase,
	saveBytesWithPrompt,
	shortStamp,
	type SaveResult,
} from '#core/util/files.js'
import {
	buildCsv,
	buildJson,
	cellToString,
	type TableExportColumn,
} from './text-formats'

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
	)
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
	)
}
