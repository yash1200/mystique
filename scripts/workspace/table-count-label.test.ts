import { describe, expect, test } from 'vitest'

import {
	approxCount,
	countLabel,
	countState,
} from '../../src/lib/features/workspace/table/count'

describe('table count label', () => {
	test('an exact total is plain', () => {
		const state = countState({ totalRows: 1234, totalKnown: true, references: [], outcome: 'exact' })
		expect(state).toEqual({ status: 'exact', total: 1234 })
		expect(countLabel(state)).toMatchObject({ text: '1,234 rows', action: null })
	})

	test('an estimate is marked and offers the exact count', () => {
		const state = countState({
			totalRows: 0,
			totalKnown: false,
			references: [],
			outcome: 'skipped',
			estimatedRows: 48_300_000,
		})
		const label = countLabel(state)
		expect(label?.text).toBe('~48.3M rows')
		expect(label?.action).toBe('count')
		expect(label?.title).toContain('Large table')
	})

	test('no estimate still offers a count; a running count can be cancelled', () => {
		const failed = countState({ totalRows: 0, totalKnown: false, references: [], outcome: 'failed' })
		expect(countLabel(failed)).toMatchObject({ text: 'Count rows', action: 'count' })
		expect(
			countLabel({ status: 'counting', cancelling: false, exact: false }),
		).toMatchObject({ text: 'Counting…', action: 'cancel' })
		expect(
			countLabel({ status: 'counting', cancelling: true, exact: true }),
		).toMatchObject({ text: 'Cancelling…', action: null })
	})

	test('approximate numbers stay readable', () => {
		expect(approxCount(950)).toBe('~950')
		expect(approxCount(9_999)).toBe('~9,999')
		expect(approxCount(12_345)).toBe('~12.3K')
		expect(approxCount(1_200_000)).toBe('~1.2M')
	})
})
