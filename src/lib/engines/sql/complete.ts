/**
 * Catalog columns → `@codemirror/lang-sql` schema namespace + extra
 * FROM-clause / snippet completions. `nestSchemas` is passed by the adapter —
 * Postgres nests, MySQL does not.
 */
import type { CatalogColumnRow } from '#core/contracts'
import {
	CompletionContext,
	snippetCompletion,
	type Completion,
	type CompletionSource,
} from '@codemirror/autocomplete'
import { syntaxTree } from '@codemirror/language'
import {
	schemaCompletionSource,
	type SQLDialect,
	type SQLNamespace,
} from '@codemirror/lang-sql'
import type { EditorState } from '@codemirror/state'

import { quoteForKind, quoteIdent, type IdentQuote } from './quote'
import { expandStarRange, scopeCompletions } from './scope'

export type SqlCompleteConfig = {
	schema: SQLNamespace
	defaultSchema?: string
}

export type SqlSchemaOptions = {
	nestSchemas?: boolean | 'auto'
	/** Catalog idents are inserted quoted. Display labels stay raw. */
	quote?: IdentQuote
}

function columnCompletion(row: CatalogColumnRow, quote: IdentQuote): Completion {
	const bits = [row.dataType].filter(Boolean)
	if (row.pk) bits.push('pk')
	if (row.nullable === false) bits.push('not null')
	return {
		label: row.column,
		type: 'property',
		detail: bits.join(' · ') || undefined,
		apply: quoteIdent(quote, row.column),
	}
}

function tableNode(
	name: string,
	kind: string,
	cols: CatalogColumnRow[],
	quote: IdentQuote,
): SQLNamespace {
	return {
		self: {
			label: name,
			type: 'type',
			detail: kind,
			apply: quoteIdent(quote, name),
		},
		children: cols.map((row) => columnCompletion(row, quote)),
	}
}

function schemaNode(
	name: string,
	children: Record<string, SQLNamespace>,
	quote: IdentQuote,
): SQLNamespace {
	return {
		self: {
			label: name,
			type: 'namespace',
			detail: 'schema',
			apply: quoteIdent(quote, name),
		},
		children,
	}
}

/**
 * Stable across a new array that holds the same columns.
 * The editor rebuilds `sql()` only when this changes.
 */
export function catalogSignature(rows: readonly CatalogColumnRow[]): string {
	let hash = 2166136261
	const mix = (text: string) => {
		for (let i = 0; i < text.length; i++) {
			hash ^= text.charCodeAt(i)
			hash = Math.imul(hash, 16777619)
		}
	}
	mix(String(rows.length))
	for (const row of rows) {
		mix(row.schema ?? '')
		mix('\0')
		mix(row.table)
		mix('\0')
		mix(row.column)
		mix('\0')
		mix(row.dataType)
		mix(row.pk ? '1' : '0')
	}
	return (hash >>> 0).toString(16)
}

/** Nest by schema when the engine uses more than one real schema (Postgres). */
export function sqlSchemaFromColumns(
	rows: CatalogColumnRow[],
	options: SqlSchemaOptions = {},
): SqlCompleteConfig {
	const quote = options.quote ?? 'postgres'
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

	const nameOwners = new Map<string, number>()
	for (const tables of bySchema.values()) {
		for (const table of tables.keys()) {
			nameOwners.set(table, (nameOwners.get(table) ?? 0) + 1)
		}
	}

	if (!nestSchemas) {
		const flat: Record<string, SQLNamespace> = {}
		for (const [schema, tables] of bySchema) {
			for (const [table, cols] of tables) {
				// Same table name in two databases must not share one column list.
				const collided = (nameOwners.get(table) ?? 0) > 1 && schema.length > 0
				const key = collided ? `${schema}.${table}` : table
				flat[key] = tableNode(table, 'table', cols, quote)
			}
		}
		return { schema: flat }
	}

	const tree: Record<string, SQLNamespace> = {}
	for (const [schema, tables] of bySchema) {
		const key = schema || 'public'
		const children: Record<string, SQLNamespace> = {}
		for (const [table, cols] of tables) {
			children[table] = tableNode(table, 'table', cols, quote)
		}
		tree[key] = schemaNode(key, children, quote)
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

function dialectSnippets(kind: SqlSnippetKind): Completion[] {
	const common = [
		snippetCompletion('SELECT ${columns} FROM ${table}', {
			label: 'SELECT * FROM',
			type: 'keyword',
			boost: 2,
		}),
		snippetCompletion('INSERT INTO ${table} (${columns}) VALUES (${values})', {
			label: 'INSERT INTO',
			type: 'keyword',
			boost: 1,
		}),
		snippetCompletion('UPDATE ${table} SET ${column} = ${value}', {
			label: 'UPDATE',
			type: 'keyword',
			boost: 1,
		}),
		snippetCompletion('DELETE FROM ${table}', {
			label: 'DELETE FROM',
			type: 'keyword',
			boost: 1,
		}),
		snippetCompletion('CREATE TABLE ${name} (\n\t${columns}\n)', {
			label: 'CREATE TABLE',
			type: 'keyword',
			boost: 1,
		}),
	]
	if (kind === 'mysql') {
		return [
			...common,
			snippetCompletion('LIMIT ${count}', { label: 'LIMIT', type: 'keyword' }),
			snippetCompletion('AUTO_INCREMENT', {
				label: 'AUTO_INCREMENT',
				type: 'keyword',
			}),
			snippetCompletion('SHOW TABLES', { label: 'SHOW TABLES', type: 'keyword' }),
		]
	}
	if (kind === 'sqlite') {
		return [
			...common,
			snippetCompletion('LIMIT ${count}', { label: 'LIMIT', type: 'keyword' }),
			snippetCompletion('PRAGMA ${name}', { label: 'PRAGMA', type: 'keyword' }),
			snippetCompletion('AUTOINCREMENT', {
				label: 'AUTOINCREMENT',
				type: 'keyword',
			}),
		]
	}
	return [
		...common,
		snippetCompletion('RETURNING ${columns}', {
			label: 'RETURNING',
			type: 'keyword',
		}),
		snippetCompletion('ILIKE ${pattern}', { label: 'ILIKE', type: 'keyword' }),
		snippetCompletion('LIMIT ${count}', { label: 'LIMIT', type: 'keyword' }),
	]
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

/** Snippets stay out of strings and comments. The syntax tree is the check. */
export function inSqlLiteralOrComment(state: EditorState, pos: number): boolean {
	let node = syntaxTree(state).resolveInner(pos, -1)
	for (;;) {
		if (/comment|string|quotedidentifier/i.test(node.name)) return true
		if (!node.parent) return false
		node = node.parent
	}
}

/**
 * Extra SQL completions on top of lang-sql's schema/keyword sources.
 *
 * lang-sql stays silent after `FROM ` until a character is typed
 * (`empty && !explicit`). This source forces the catalog table list there,
 * and adds dialect snippets outside strings and comments. `table.` stays
 * with lang-sql. Alias and CTE columns are filled here.
 */
export function sqlCatalogCompletion(
	rows: CatalogColumnRow[],
	options: SqlCatalogCompleteOptions,
): CompletionSource {
	const quote = quoteForKind(options.kind)
	const cfg = sqlSchemaFromColumns(rows, {
		nestSchemas: options.nestSchemas,
		quote,
	})
	const schemaSrc = schemaCompletionSource({
		schema: cfg.schema,
		defaultSchema: cfg.defaultSchema,
		dialect: options.dialect,
	})
	const snippets = dialectSnippets(options.kind)
	const mysql = options.kind === 'mysql'

	return (context) => {
		if (inSqlLiteralOrComment(context.state, context.pos)) return null
		const doc = context.state.doc.toString()
		const star = expandStarRange(doc, context.pos, rows, quote, mysql)
		if (star && context.pos >= star.from && context.pos <= star.to) {
			return {
				from: star.from,
				to: star.to,
				options: [
					{
						label: 'Expand *',
						type: 'keyword',
						detail: star.insert,
						apply: star.insert,
						boost: 99,
					},
				],
			}
		}

		const before = context.state.sliceDoc(
			Math.max(0, context.pos - 800),
			context.pos,
		)
		const kind = tableCompletionKind(before)

		if (kind === 'column') {
			return scopeCompletions(doc, context.pos, rows, quote, mysql)
		}

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
		const scope = scopeCompletions(doc, context.pos, rows, quote, mysql)
		const optionsOut = [
			...(scope?.options ?? []).filter((option) =>
				option.label.toLowerCase().startsWith(prefix),
			),
			...snippets.filter((option) =>
				option.label.toLowerCase().startsWith(prefix),
			),
		]
		if (optionsOut.length === 0) return null
		return {
			from: scope && prefix.includes('.') ? scope.from : (word?.from ?? context.pos),
			options: optionsOut,
			validFor: /^[\w.]*$/,
		}
	}
}
