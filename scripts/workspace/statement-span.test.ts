import { describe, expect, it } from 'vitest'
import {
	splitStatementSpans,
	statementAt,
	textToRun,
} from '../../src/lib/features/workspace/query/statement-span'

describe('statement spans', () => {
	it('runs the statement under the cursor', () => {
		const sql = 'SELECT 1; SELECT 2;'
		const cursor = sql.indexOf('SELECT 2') + 3
		const hit = statementAt(splitStatementSpans(sql, 'postgres'), cursor)
		expect(hit?.text).toBe('SELECT 2')
		expect(
			textToRun(sql, 'postgres', cursor, cursor, false),
		).toBe('SELECT 2')
	})

	it('keeps a semicolon inside dollar quotes in one range', () => {
		const sql = 'SELECT $$ a; b $$; SELECT 2'
		const spans = splitStatementSpans(sql, 'postgres')
		expect(spans).toHaveLength(2)
		expect(sql.slice(spans[0]!.start, spans[0]!.end)).toBe(spans[0]!.text)
		expect(spans[0]!.text).toContain('a; b')
		expect(spans[0]!.text).not.toContain('SELECT 2')
		expect(spans[1]!.text).toBe('SELECT 2')
	})

	it('runs a selection even when it is not a full statement', () => {
		const sql = 'SELECT 1; SELECT 2;'
		const from = sql.indexOf('LECT 2')
		const to = from + 'LECT 2'.length
		expect(textToRun(sql, 'postgres', to, from, false)).toBe('LECT 2')
	})

	it('shift-enter sends the whole script', () => {
		const sql = 'SELECT 1; SELECT 2;'
		expect(textToRun(sql, 'postgres', 0, 0, true)).toBe(sql)
	})
})
