/**
 * Catalog columns → `@codemirror/lang-sql` schema namespace + extra
 * FROM-clause / snippet completions. `nestSchemas` is passed by the adapter —
 * Postgres nests, MySQL does not.
 */
import type { CatalogColumnRow } from '#core/contracts'
import {
	CompletionContext,
	type Completion,
	type CompletionSource,
} from '@codemirror/autocomplete'
import {
	schemaCompletionSource,
	type SQLDialect,
	type SQLNamespace,
} from '@codemirror/lang-sql'

export type SqlCompleteConfig = {
	schema: SQLNamespace
	defaultSchema?: string
}

function colOption(row: CatalogColumnRow): Completion {
	const bits = [row.dataType].filter(Boolean)
	if (row.pk) bits.push('pk')
	if (row.nullable === false) bits.push('not null')
	return {
		label: row.column,
		type: 'property',
		detail: bits.join(' · ') || undefined,
	}
}

function tableChildren(cols: CatalogColumnRow[]): SQLNamespace {
	return cols.map(colOption)
}

function tableEntry(
	name: string,
	kind: string,
	cols: CatalogColumnRow[],
): SQLNamespace {
	return {
		self: { label: name, type: 'type', detail: kind },
		children: tableChildren(cols),
	}
}

/** Nest by schema when the engine uses more than one real schema (Postgres). */
export function sqlSchemaFromColumns(
	rows: CatalogColumnRow[],
	options: { nestSchemas?: boolean | 'auto' } = {},
): SqlCompleteConfig {
	const bySchema = new Map<string, Map<string, CatalogColumnRow[]>>()
	for (const row of rows) {
		const table = row.table?.trim()
		const column = row.column?.trim()
		if (!table || !column) continue
		const schema = (row.schema ?? '').trim()
		let tables = bySchema.get(schema)
		if (!tables) {
			tables = new Map()
			bySchema.set(schema, tables)
		}
		const list = tables.get(table) ?? []
		list.push(row)
		tables.set(table, list)
	}

	const schemas = [...bySchema.keys()]
	const nest = options.nestSchemas ?? 'auto'
	const nestSchemas =
		nest === true ||
		(nest === 'auto' && schemas.filter((s) => s && s !== 'public').length > 1)

	if (!nestSchemas) {
		const flat: Record<string, SQLNamespace> = {}
		for (const tables of bySchema.values()) {
			for (const [table, cols] of tables) {
				flat[table] = tableEntry(table, 'table', cols)
			}
		}
		return { schema: flat }
	}

	const tree: Record<string, SQLNamespace> = {}
	for (const [schema, tables] of bySchema) {
		const key = schema || 'public'
		const children: Record<string, SQLNamespace> = {}
		for (const [table, cols] of tables) {
			children[table] = tableEntry(table, 'table', cols)
		}
		tree[key] = {
			self: { label: key, type: 'namespace', detail: 'schema' },
			children,
		}
	}
	const defaultSchema = tree.public
		? 'public'
		: schemas.find((s) => s.length > 0) || undefined
	return { schema: tree, defaultSchema }
}

export type SqlSnippetKind = 'postgresql' | 'mysql' | 'sqlite'

export type SqlCatalogCompleteOptions = {
	nestSchemas?: boolean | 'auto'
	dialect: SQLDialect
	kind: SqlSnippetKind
}

const COMMON_SNIPPETS: Completion[] = [
	{ label: 'SELECT * FROM', type: 'keyword', boost: 2, apply: 'SELECT * FROM ' },
	{ label: 'INSERT INTO', type: 'keyword', boost: 1, apply: 'INSERT INTO ' },
	{ label: 'UPDATE', type: 'keyword', boost: 1, apply: 'UPDATE ' },
	{ label: 'DELETE FROM', type: 'keyword', boost: 1, apply: 'DELETE FROM ' },
	{ label: 'CREATE TABLE', type: 'keyword', boost: 1, apply: 'CREATE TABLE ' },
]

const DIALECT_SNIPPETS: Record<SqlSnippetKind, Completion[]> = {
	postgresql: [
		{ label: 'RETURNING', type: 'keyword', apply: 'RETURNING ' },
		{ label: 'ILIKE', type: 'keyword' },
		{ label: 'LIMIT', type: 'keyword', apply: 'LIMIT ' },
	],
	mysql: [
		{ label: 'LIMIT', type: 'keyword', apply: 'LIMIT ' },
		{ label: 'AUTO_INCREMENT', type: 'keyword' },
		{ label: 'SHOW TABLES', type: 'keyword', apply: 'SHOW TABLES' },
	],
	sqlite: [
		{ label: 'LIMIT', type: 'keyword', apply: 'LIMIT ' },
		{ label: 'PRAGMA', type: 'keyword', apply: 'PRAGMA ' },
		{ label: 'AUTOINCREMENT', type: 'keyword' },
	],
}

/** Where the cursor is, for catalog vs snippet completions. */
export function tableCompletionKind(
	before: string,
): 'table' | 'column' | 'other' {
	// Keep a trailing space so `FROM ` still counts as a table slot.
	const t = before.replace(/--[^\n]*/g, '').replace(/\s+/g, ' ')
	if (/\.(["'`][\w$]*|[\w$]*)$/.test(t) || /\.\s*$/.test(t)) return 'column'
	if (/\b(from|join|into|update|table)\s+(only\s+)?[\w."'`]*$/i.test(t)) {
		return 'table'
	}
	return 'other'
}

/**
 * Extra SQL completions on top of lang-sql's schema/keyword sources.
 *
 * lang-sql stays silent after `FROM ` until a character is typed
 * (`empty && !explicit`). This source forces the catalog table list there,
 * and adds a small dialect snippet set. Dotted `table.` / alias columns stay
 * with lang-sql so they are not duplicated.
 */
export function sqlCatalogCompletion(
	rows: CatalogColumnRow[],
	options: SqlCatalogCompleteOptions,
): CompletionSource {
	const cfg = sqlSchemaFromColumns(rows, { nestSchemas: options.nestSchemas })
	const schemaSrc = schemaCompletionSource({
		schema: cfg.schema,
		defaultSchema: cfg.defaultSchema,
		dialect: options.dialect,
	})
	const snippets = [...COMMON_SNIPPETS, ...DIALECT_SNIPPETS[options.kind]]

	return (context) => {
		const before = context.state.sliceDoc(
			Math.max(0, context.pos - 800),
			context.pos,
		)
		const kind = tableCompletionKind(before)

		if (kind === 'column') return null

		if (kind === 'table') {
			const word = context.matchBefore(/[^\s.]*$/)
			if (word && word.from !== word.to) return null
			return schemaSrc(
				new CompletionContext(context.state, context.pos, true),
			)
		}

		const word = context.matchBefore(/\w*/)
		if (word && word.from === word.to && !context.explicit) return null
		const prefix = (
			word ? context.state.sliceDoc(word.from, context.pos) : ''
		).toLowerCase()
		const optionsOut = snippets.filter((s) =>
			s.label.toLowerCase().startsWith(prefix),
		)
		if (optionsOut.length === 0) return null
		return {
			from: word?.from ?? context.pos,
			options: optionsOut,
			validFor: /^\w*$/,
		}
	}
}
