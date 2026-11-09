import { readFileSync } from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'
import { splitMongoStatements } from '../../src/lib/engines/mongodb/statements'
import {
	splitDialectFor,
	splitStatementSpans,
	statementAt,
	submissionStart,
	textToRun,
} from '../../src/lib/features/workspace/query/editor/statement-span'

type Case = { name: string; text: string; statements: string[] }

/** The corpus `shell/split.rs` is tested against. */
const cases: Case[] = JSON.parse(
	readFileSync(
		path.resolve(
			__dirname,
			'../../crates/plugins/mystique-plugin-mongodb/tests/fixtures/statements/cases.json',
		),
		'utf8',
	),
)

describe('Mongo statement split matches the backend corpus', () => {
	it('has the whole corpus', () => {
		expect(cases.length).toBeGreaterThanOrEqual(20)
	})

	for (const c of cases) {
		it(c.name, () => {
			const spans = splitMongoStatements(c.text)
			expect(spans.map((s) => s.text)).toEqual(c.statements)
			for (const s of spans) expect(c.text.slice(s.start, s.end)).toBe(s.text)
		})
	}
})

describe('Mongo run at cursor', () => {
	const doc = 'db.orders\n  .find({})\n  .limit(5)\n\n// next\ndb.users.deleteMany({ a: ";" })'

	it('uses the shell splitter for the mongodb engine', () => {
		expect(splitDialectFor('mongodb')).toBe('mongo')
		expect(splitStatementSpans(doc, 'mongo').map((s) => s.text)).toEqual([
			'db.orders\n  .find({})\n  .limit(5)',
			'db.users.deleteMany({ a: ";" })',
		])
	})

	it('runs the whole chain from any of its lines', () => {
		const cursor = doc.indexOf('.limit')
		expect(textToRun(doc, 'mongo', cursor, cursor, false)).toBe(
			'db.orders\n  .find({})\n  .limit(5)',
		)
		expect(submissionStart(doc, 'mongo', cursor, cursor, false)).toBe(0)
	})

	it('runs the statement the cursor is in, not the one before it', () => {
		const cursor = doc.indexOf('deleteMany') + 3
		const hit = statementAt(splitStatementSpans(doc, 'mongo'), cursor)
		expect(hit?.text).toBe('db.users.deleteMany({ a: ";" })')
		expect(submissionStart(doc, 'mongo', cursor, cursor, false)).toBe(
			doc.indexOf('db.users'),
		)
	})

	it('Run all still sends the whole buffer', () => {
		expect(textToRun(`  ${doc}\n`, 'mongo', 0, 0, true)).toBe(doc)
	})
})
