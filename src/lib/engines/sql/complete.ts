/**
 * Catalog columns → `@codemirror/lang-sql` schema namespace.
 * `nestSchemas` is passed by the adapter — Postgres nests, MySQL does not.
 */
import type { CatalogColumnRow } from '$core/contracts'
import type { Completion } from '@codemirror/autocomplete'
import type { SQLNamespace } from '@codemirror/lang-sql'

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
