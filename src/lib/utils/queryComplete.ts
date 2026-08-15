/**
 * Query-editor completions from live plugin catalog columns.
 * SQL uses @codemirror/lang-sql schema; Mongo uses a custom source.
 */
import type { CatalogColumnRow } from '$lib/api/engines'
import type { Completion, CompletionSource } from '@codemirror/autocomplete'
import {
	MySQL,
	PostgreSQL,
	SQLite,
	StandardSQL,
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
export function sqlCompleteFromColumns(
	engine: string,
	rows: CatalogColumnRow[],
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
	const e = engine.toLowerCase()
	const nestSchemas =
		e === 'postgres' ||
		(schemas.filter((s) => s && s !== 'public').length > 1 && e !== 'mysql')

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

export function sqlDialectForEngine(engine: string): SQLDialect {
	switch (engine.toLowerCase()) {
		case 'postgres':
		case 'postgresql':
			return PostgreSQL
		case 'mysql':
		case 'mariadb':
			return MySQL
		case 'sqlite':
			return SQLite
		default:
			return StandardSQL
	}
}

const MONGO_METHODS: Completion[] = [
	'find',
	'findOne',
	'aggregate',
	'countDocuments',
	'estimatedDocumentCount',
	'distinct',
	'insertOne',
	'insertMany',
	'updateOne',
	'updateMany',
	'replaceOne',
	'deleteOne',
	'deleteMany',
	'bulkWrite',
].map((label) => ({ label, type: 'function' }))

const MONGO_TOP: Completion[] = [
	{ label: 'db', type: 'variable', detail: 'database' },
	{ label: 'use', type: 'keyword' },
	{ label: 'show', type: 'keyword' },
	{ label: 'find', type: 'function' },
	{ label: 'aggregate', type: 'function' },
]

/** Collection + field completions for the Mongo / JS query editor. */
export function mongoCompletionSource(
	rows: CatalogColumnRow[],
): CompletionSource {
	const byColl = new Map<string, Completion[]>()
	for (const row of rows) {
		const coll = row.table?.trim()
		const field = row.column?.trim()
		if (!coll || !field) continue
		const list = byColl.get(coll) ?? []
		if (!list.some((c) => c.label === field)) {
			list.push({
				label: field,
				type: 'property',
				detail: row.dataType || undefined,
			})
		}
		byColl.set(coll, list)
	}
	const collections: Completion[] = [...byColl.keys()].map((name) => ({
		label: name,
		type: 'class',
		detail: 'collection',
	}))

	return (context) => {
		const word = context.matchBefore(/[\w$]*/)
		if (word && word.from === word.to && !context.explicit) return null
		const line = context.state.doc.lineAt(context.pos)
		const prefix = line.text.slice(0, context.pos - line.from)

		const method = prefix.match(/\bdb\.(\w+)\.(\w*)$/)
		if (method) {
			return {
				from: context.pos - method[2].length,
				options: MONGO_METHODS,
				validFor: /^\w*$/,
			}
		}
		const coll = prefix.match(/\bdb\.(\w*)$/)
		if (coll) {
			return {
				from: context.pos - coll[1].length,
				options: collections,
				validFor: /^\w*$/,
			}
		}
		if (/\{[^}]*$/.test(prefix)) {
			const fromFind = prefix.match(/\bdb\.(\w+)\.[^}]*\{[^}]*$/)
			const fields = fromFind
				? (byColl.get(fromFind[1]) ?? [])
				: [...byColl.values()].flat()
			if (fields.length > 0) {
				return {
					from: word?.from ?? context.pos,
					options: fields,
					validFor: /^[\w$]*$/,
				}
			}
		}
		return {
			from: word?.from ?? context.pos,
			options: [...MONGO_TOP, ...collections],
			validFor: /^\w*$/,
		}
	}
}
