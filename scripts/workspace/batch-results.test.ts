import { describe, expect, it } from 'vitest'
import {
	initialActiveIndex,
	statementChipLabel,
	statementChipTitle,
} from '../../src/lib/features/workspace/query/results/batch-view'

describe('batch result strip', () => {
	it('keeps one chip for a single statement', () => {
		const results = [{ index: 0, setIndex: 0 }]
		expect(statementChipLabel(results, results[0]!)).toBe('1')
		expect(initialActiveIndex(results, null)).toBe(0)
	})

	it('numbers statements and call result sets', () => {
		const results = [
			{ index: 0, setIndex: 0 },
			{ index: 0, setIndex: 1 },
			{ index: 1, setIndex: 0 },
		]
		expect(statementChipLabel(results, results[0]!)).toBe('1.1')
		expect(statementChipLabel(results, results[1]!)).toBe('1.2')
		expect(statementChipLabel(results, results[2]!)).toBe('2')
	})

	it('opens the statement that stopped the batch', () => {
		const results = [
			{ index: 0, error: null },
			{ index: 1, error: 'no such table' },
		]
		expect(initialActiveIndex(results, 1)).toBe(1)
		expect(initialActiveIndex(results, null)).toBe(1)
	})

	it('titles a chip from the submitted statement', () => {
		const sql = 'SELECT 1; SELECT 2'
		expect(statementChipTitle(sql, { start: 10, end: sql.length })).toBe('SELECT 2')
	})
})
