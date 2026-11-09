import { describe, expect, it } from 'vitest'
import { splitMongoStatements } from '../../src/lib/engines/mongodb/statements'
import {
	statementModelFor,
	textToRun,
} from '../../src/lib/features/workspace/query/editor/statement-span'

const texts = (src: string) => splitMongoStatements(src).map((span) => span.text)

describe('Mongo shell statements', () => {
	it('splits on newlines between complete calls', () => {
		expect(texts('db.a.find()\ndb.b.countDocuments({})\nshow collections')).toEqual([
			'db.a.find()',
			'db.b.countDocuments({})',
			'show collections',
		])
	})

	it('keeps a chained call on the next line', () => {
		const src = 'db.users\n  .find({ age: { $gt: 1 } })\n  // newest first\n  .sort({ at: -1 })\n  .limit(5)\ndb.x.find()'
		expect(texts(src)).toEqual([
			'db.users\n  .find({ age: { $gt: 1 } })\n  // newest first\n  .sort({ at: -1 })\n  .limit(5)',
			'db.x.find()',
		])
	})

	it('keeps multi-line documents and pipelines whole', () => {
		const src = 'db.c.aggregate([\n  { $match: { a: 1 } },\n  { $group: { _id: "$b" } }\n])\n{ a: 1 }'
		expect(texts(src)).toEqual([
			'db.c.aggregate([\n  { $match: { a: 1 } },\n  { $group: { _id: "$b" } }\n])',
			'{ a: 1 }',
		])
	})

	it('ignores semicolons, braces and newlines in strings, templates and comments', () => {
		const src = [
			'db.c.find({ s: "a;b}", t: \'it\\\'s )\' }); db.d.find()',
			'db.e.insertOne({ note: `line one;',
			'line two ${"}"} }` }) /* ; { */',
			'// db.never.find()',
		].join('\n')
		expect(texts(src)).toEqual([
			'db.c.find({ s: "a;b}", t: \'it\\\'s )\' })',
			'db.d.find()',
			'db.e.insertOne({ note: `line one;\nline two ${"}"} }` }) /* ; { */',
		])
	})

	it('continues after a trailing operator or comma', () => {
		expect(texts('db.c.find({},\n  { a: 1 })\ndb.c.find()')).toEqual([
			'db.c.find({},\n  { a: 1 })',
			'db.c.find()',
		])
		expect(texts('db.c\n.find()')).toEqual(['db.c\n.find()'])
		expect(texts('db.c.\nfind()')).toEqual(['db.c.\nfind()'])
	})

	it('drops comment-only chunks and keeps a leading comment with its statement', () => {
		expect(texts('// all users\ndb.users.find()\n\n/* end */\n;')).toEqual([
			'// all users\ndb.users.find()',
		])
	})

	it('recovers from an unterminated string at the end of its line', () => {
		expect(texts('db.c.find({ a: "oops })\ndb.d.find()')).toEqual([
			'db.c.find({ a: "oops })\ndb.d.find()',
		])
		expect(texts('db.c.find("oops)\n\ndb.d.find()')).toEqual([
			'db.c.find("oops)\n\ndb.d.find()',
		])
	})

	it('a stray closer does not swallow the rest', () => {
		expect(texts('db.c.find())\ndb.d.find()')).toEqual(['db.c.find())', 'db.d.find()'])
	})

	it('run at cursor on a Mongo tab sends one statement', () => {
		const src = 'db.a.find()\ndb.b.find({ x: 1 })'
		const model = statementModelFor('mongodb', 'mql')
		expect(model).toBe('mongo')
		const cursor = src.indexOf('x: 1')
		expect(textToRun(src, model, cursor, cursor, false)).toBe('db.b.find({ x: 1 })')
		// The SQL model would have sent both lines.
		expect(textToRun(src, 'generic', cursor, cursor, false)).toBe(src)
	})
})
