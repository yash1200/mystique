/** Collection + field completions for the Mongo / JS query editor. */
import type { CatalogColumnRow } from '#core/contracts'
import type { Completion, CompletionSource } from '@codemirror/autocomplete'

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
