/**
 * Copy a grid selection as SQL text.
 * Identifiers follow the plugin quote style. Values are literals, never spliced raw.
 */
import { isLargeCell, largeCellText } from './large-cell'

export type IdentStyle = 'postgres' | 'mysql'

export type CopyColumn = {
	name: string
	dataType?: string
	pk?: boolean
}

export function identStyleFor(engine: string, sqlDialect: string): IdentStyle {
	const blob = `${engine} ${sqlDialect}`.toLowerCase()
	if (
		blob.includes('mysql') ||
		blob.includes('mariadb') ||
		blob.includes('tidb')
	) {
		return 'mysql'
	}
	return 'postgres'
}

export function quoteIdent(style: IdentStyle, id: string): string {
	if (style === 'mysql') return `\`${id.replace(/`/g, '``')}\``
	return `"${id.replace(/"/g, '""')}"`
}

function qualifyTable(
	style: IdentStyle,
	database: string | null | undefined,
	schema: string | null | undefined,
	table: string,
): string {
	const name = quoteIdent(style, table)
	if (style === 'mysql') {
		const db = schema?.trim() || database?.trim()
		return db ? `${quoteIdent(style, db)}.${name}` : name
	}
	const schemaName = schema?.trim()
	return schemaName ? `${quoteIdent(style, schemaName)}.${name}` : name
}

function sqlStringLiteral(style: IdentStyle, value: string): string {
	if (style === 'mysql') {
		let out = "'"
		for (const char of value) {
			if (char === '\\') out += '\\\\'
			else if (char === "'") out += "''"
			else if (char === '\0') out += '\\0'
			else out += char
		}
		return `${out}'`
	}
	return `'${value.replace(/'/g, "''")}'`
}

function isNumericType(dataType: string): boolean {
	const type = dataType.toLowerCase()
	return (
		type.includes('int') ||
		type.includes('serial') ||
		type.includes('numeric') ||
		type.includes('decimal') ||
		type.includes('float') ||
		type.includes('double') ||
		type.includes('real') ||
		type.includes('money') ||
		type.includes('number') ||
		type.includes('oid')
	)
}

function isBoolType(dataType: string): boolean {
	const type = dataType.toLowerCase()
	return (
		type === 'bool' ||
		type === 'boolean' ||
		type === 'bit' ||
		type === 'bit(1)' ||
		type.includes('tinyint(1)')
	)
}

function isPlainNumber(value: string): boolean {
	return /^[+-]?(?:\d+\.?\d*|\.\d+)$/.test(value)
}

function boolSql(style: IdentStyle, value: boolean): string {
	if (style === 'mysql') return value ? '1' : '0'
	return value ? 'TRUE' : 'FALSE'
}

/** Bound-safe literal. A quote in the value stays inside the string. */
export function sqlLiteral(
	style: IdentStyle,
	value: unknown,
	dataType = '',
): string {
	if (isLargeCell(value)) return sqlStringLiteral(style, largeCellText(value))
	if (value == null) return 'NULL'
	if (typeof value === 'boolean') return boolSql(style, value)
	if (typeof value === 'number') {
		return Number.isFinite(value) ? String(value) : sqlStringLiteral(style, String(value))
	}
	if (typeof value === 'string') {
		if (isBoolType(dataType)) {
			const flag = ['true', '1', 'yes', 't'].includes(value.toLowerCase())
			return boolSql(style, flag)
		}
		const trimmed = value.trim()
		if (isNumericType(dataType) && trimmed && isPlainNumber(trimmed)) return trimmed
		return sqlStringLiteral(style, value)
	}
	try {
		return sqlStringLiteral(style, JSON.stringify(value))
	} catch {
		return sqlStringLiteral(style, String(value))
	}
}

function targetName(
	style: IdentStyle,
	table: string,
	database?: string | null,
	schema?: string | null,
): string {
	return qualifyTable(style, database, schema, table)
}

export function buildInsertSql(
	style: IdentStyle,
	table: string,
	columns: CopyColumn[],
	rows: unknown[][],
	database?: string | null,
	schema?: string | null,
): string {
	if (columns.length === 0 || rows.length === 0) return ''
	const into = targetName(style, table, database, schema)
	const names = columns.map((column) => quoteIdent(style, column.name)).join(', ')
	return rows
		.map((row) => {
			const values = columns
				.map((column, index) => sqlLiteral(style, row[index], column.dataType ?? ''))
				.join(', ')
			return `INSERT INTO ${into} (${names}) VALUES (${values});`
		})
		.join('\n')
}

export function buildUpdateSql(
	style: IdentStyle,
	table: string,
	columns: CopyColumn[],
	rows: unknown[][],
	database?: string | null,
	schema?: string | null,
): string {
	if (columns.length === 0 || rows.length === 0) return ''
	const into = targetName(style, table, database, schema)
	const pks = columns.filter((column) => column.pk)
	const keys = pks.length > 0 ? pks : columns
	const sets = pks.length > 0 ? columns.filter((column) => !column.pk) : columns
	const assigned = sets.length > 0 ? sets : columns
	return rows
		.map((row) => {
			const setSql = assigned
				.map((column) => {
					const index = columns.indexOf(column)
					return `${quoteIdent(style, column.name)} = ${sqlLiteral(style, row[index], column.dataType ?? '')}`
				})
				.join(', ')
			const whereSql = keys
				.map((column) => {
					const index = columns.indexOf(column)
					const ident = quoteIdent(style, column.name)
					const value = row[index]
					if (value == null) return `${ident} IS NULL`
					return `${ident} = ${sqlLiteral(style, value, column.dataType ?? '')}`
				})
				.join(' AND ')
			return `UPDATE ${into} SET ${setSql} WHERE ${whereSql};`
		})
		.join('\n')
}

export function buildInList(
	style: IdentStyle,
	columns: CopyColumn[],
	rows: unknown[][],
): string {
	if (columns.length === 0 || rows.length === 0) return ''
	return columns
		.map((column, index) => {
			const values = rows
				.map((row) => sqlLiteral(style, row[index], column.dataType ?? ''))
				.join(', ')
			return `${quoteIdent(style, column.name)} IN (${values})`
		})
		.join('\n')
}

export function buildWhereSql(
	style: IdentStyle,
	columns: CopyColumn[],
	rows: unknown[][],
): string {
	if (columns.length === 0 || rows.length === 0) return ''
	if (columns.length === 1) return `WHERE ${buildInList(style, columns, rows)}`
	const ors = rows.map((row) => {
		const ands = columns.map((column, index) => {
			const ident = quoteIdent(style, column.name)
			const value = row[index]
			if (value == null) return `${ident} IS NULL`
			return `${ident} = ${sqlLiteral(style, value, column.dataType ?? '')}`
		})
		return ands.length === 1 ? ands[0] : `(${ands.join(' AND ')})`
	})
	return `WHERE ${ors.join(' OR ')}`
}
