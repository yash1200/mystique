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
	'findOneAndUpdate',
	'bulkWrite',
].map((label) => ({ label, type: 'function' }))

const AGGREGATE_STAGES: Completion[] = [
	'$match',
	'$group',
	'$limit',
	'$project',
	'$sort',
	'$skip',
	'$unwind',
	'$lookup',
	'$addFields',
	'$set',
	'$unset',
	'$count',
	'$facet',
	'$bucket',
	'$sortByCount',
	'$replaceRoot',
	'$replaceWith',
	'$out',
	'$merge',
	'$unionWith',
	'$sample',
	'$redact',
	'$geoNear',
].map((label) => ({ label, type: 'keyword', detail: 'stage' }))

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
		const before = context.state.sliceDoc(0, context.pos)
		const line = context.state.doc.lineAt(context.pos)
		const prefix = line.text.slice(0, context.pos - line.from)
		if (aggregateStageLabels(before)) {
			return {
				from: word?.from ?? context.pos,
				options: AGGREGATE_STAGES,
				validFor: /^[\w$]*$/,
			}
		}

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

/** Stage names when the caret is on a pipeline stage key. */
export function aggregateStageLabels(prefix: string): string[] | null {
	if (!insideAggregate(prefix) || !stagePosition(prefix)) return null
	return AGGREGATE_STAGES.map((stage) => stage.label)
}

/** True once a real `.aggregate(` is still open. Quoted text does not count. */
export function insideAggregate(prefix: string): boolean {
	return openAggregateCall(prefix) !== null
}

/** The caret is on a pipeline stage key, not a field inside `$match`. */
export function stagePosition(prefix: string): boolean {
	const open = openAggregateCall(prefix)
	if (open === null) return false
	let arrayDepth = 0
	let objectDepth = 0
	let atStageKey = false
	let key = ''
	let quote: string | null = null
	let escaped = false
	let lineComment = false
	let blockComment = false
	for (let i = open + 1; i < prefix.length; i++) {
		const c = prefix.charAt(i)
		const next = prefix.charAt(i + 1)
		if (lineComment) {
			if (c === '\n') lineComment = false
			continue
		}
		if (blockComment) {
			if (c === '*' && next === '/') {
				blockComment = false
				i += 1
			}
			continue
		}
		if (quote) {
			if (escaped) {
				escaped = false
				continue
			}
			if (c === '\\') {
				escaped = true
				continue
			}
			if (c === quote) quote = null
			continue
		}
		if (c === '/' && next === '/') {
			lineComment = true
			i += 1
			continue
		}
		if (c === '/' && next === '*') {
			blockComment = true
			i += 1
			continue
		}
		if (c === '"' || c === "'" || c === '`') {
			quote = c
			continue
		}
		if (c === '[') {
			arrayDepth += 1
			continue
		}
		if (c === ']') {
			arrayDepth = Math.max(0, arrayDepth - 1)
			continue
		}
		if (c === '{') {
			objectDepth += 1
			atStageKey = arrayDepth >= 1 && objectDepth === 1
			key = ''
			continue
		}
		if (c === '}') {
			objectDepth = Math.max(0, objectDepth - 1)
			atStageKey = false
			key = ''
			continue
		}
		if (c === ':') {
			if (objectDepth === 1) atStageKey = false
			continue
		}
		if (c === ',') {
			if (arrayDepth >= 1 && objectDepth === 1) {
				atStageKey = true
				key = ''
			}
			continue
		}
		if (atStageKey && objectDepth === 1 && !/\s/.test(c)) key += c
	}
	return (
		arrayDepth >= 1 &&
		objectDepth === 1 &&
		atStageKey &&
		(key === '' || /^\$[\w]*$/.test(key))
	)
}

function openAggregateCall(prefix: string): number | null {
	let quote: string | null = null
	let escaped = false
	let lineComment = false
	let blockComment = false
	let depth = 0
	let aggregateAt: number | null = null
	for (let i = 0; i < prefix.length; i++) {
		const c = prefix.charAt(i)
		const next = prefix.charAt(i + 1)
		if (lineComment) {
			if (c === '\n') lineComment = false
			continue
		}
		if (blockComment) {
			if (c === '*' && next === '/') {
				blockComment = false
				i += 1
			}
			continue
		}
		if (quote) {
			if (escaped) {
				escaped = false
				continue
			}
			if (c === '\\') {
				escaped = true
				continue
			}
			if (c === quote) quote = null
			continue
		}
		if (c === '/' && next === '/') {
			lineComment = true
			i += 1
			continue
		}
		if (c === '/' && next === '*') {
			blockComment = true
			i += 1
			continue
		}
		if (c === '"' || c === "'" || c === '`') {
			quote = c
			continue
		}
		if (c === '(') {
			if (depth === 0 && precededByAggregate(prefix, i)) aggregateAt = i
			depth += 1
			continue
		}
		if (c === ')') {
			depth = Math.max(0, depth - 1)
			if (depth === 0) aggregateAt = null
		}
	}
	return aggregateAt
}

function precededByAggregate(prefix: string, paren: number): boolean {
	let end = paren
	while (end > 0 && /\s/.test(prefix.charAt(end - 1))) end -= 1
	const start = end - 'aggregate'.length
	if (start < 1) return false
	if (prefix.slice(start, end).toLowerCase() !== 'aggregate') return false
	let dot = start - 1
	while (dot >= 0 && /\s/.test(prefix.charAt(dot))) dot -= 1
	return prefix.charAt(dot) === '.'
}
