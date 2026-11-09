import { describe, expect, it } from 'vitest'
import type { WriteGuardPrompt, WriteRowPreview } from '../../src/lib/core/contracts/engines'
import { hasProduction } from '../../src/lib/features/connections/environments'
import { guardCopy } from '../../src/lib/features/workspace/session/guard-copy'
import {
	isNewColumn,
	previewCell,
	previewIsEmpty,
	previewSummary,
} from '../../src/lib/features/workspace/session/row-preview'
import {
	deadlineFrom,
	effectiveReadOnly,
	formatCountdown,
	TIMED_UNLOCK_CHOICES,
} from '../../src/lib/features/workspace/session/timed-unlock'

describe('production', () => {
	it('follows the connection environment only', () => {
		expect(hasProduction({ environment: 'production' })).toBe(true)
		expect(hasProduction({ environment: 'staging' })).toBe(false)
		expect(hasProduction({ environment: 'local' })).toBe(false)
	})
})

describe('timed write unlock', () => {
	it('offers one and five minutes', () => {
		expect(TIMED_UNLOCK_CHOICES.map((c) => c.seconds)).toEqual([60, 300])
	})

	it('counts down in minutes and seconds', () => {
		expect(formatCountdown(300_000)).toBe('5:00')
		expect(formatCountdown(61_001)).toBe('1:02')
		expect(formatCountdown(999)).toBe('0:01')
		expect(formatCountdown(-5)).toBe('0:00')
	})

	it('takes the deadline from the backend remaining time', () => {
		expect(deadlineFrom(60_000, 1_000)).toBe(61_000)
		expect(deadlineFrom(null, 1_000)).toBeNull()
		expect(deadlineFrom(0, 1_000)).toBeNull()
	})

	it('lifts read-only only while the window runs', () => {
		expect(effectiveReadOnly(true, null, 10)).toBe(true)
		expect(effectiveReadOnly(true, 20, 10)).toBe(false)
		expect(effectiveReadOnly(true, 20, 20)).toBe(true)
		expect(effectiveReadOnly(false, null, 10)).toBe(false)
	})
})

function preview(extra: Partial<WriteRowPreview>): WriteRowPreview {
	return {
		available: true,
		columns: [],
		rows: [],
		newColumns: [],
		truncated: false,
		...extra,
	}
}

describe('affected-row preview', () => {
	it('says how many rows change and how many are shown', () => {
		expect(previewSummary(preview({ action: 'update', total: 1, rows: [[1]] }))).toBe(
			'1 row would be updated.',
		)
		expect(
			previewSummary(preview({ action: 'delete', total: 120, rows: [[1], [2]] })),
		).toBe('120 rows would be deleted. Showing the first 2.')
		expect(previewSummary(preview({ action: 'insert', total: 2, rows: [[1], [2]] }))).toBe(
			'2 rows would be inserted.',
		)
	})

	it('makes zero rows obvious', () => {
		const none = preview({ action: 'delete', total: 0 })
		expect(previewSummary(none)).toContain('0 rows')
		expect(previewIsEmpty(none)).toBe(true)
	})

	it('explains an unavailable preview, a failed read and a missing count', () => {
		expect(
			previewSummary(preview({ available: false, reason: 'Row preview covers one table.' })),
		).toBe('Preview unavailable. Row preview covers one table.')
		expect(previewSummary(preview({ action: 'update', error: 'permission denied' }))).toBe(
			'Preview failed: permission denied',
		)
		expect(
			previewSummary(
				preview({
					action: 'update',
					rows: [[1]],
					truncated: true,
					note: 'Counting took longer than 3 seconds.',
				}),
			),
		).toBe('1 row or more would be updated. Counting took longer than 3 seconds.')
	})

	it('renders cells as text and marks the new columns', () => {
		expect(previewCell(null)).toBe('NULL')
		expect(previewCell('<b>x</b>')).toBe('<b>x</b>')
		expect(previewCell({ $large: true, preview: 'abc' })).toBe('abc…')
		expect(previewCell({ a: 1 })).toBe('{"a":1}')
		const p = preview({ newColumns: ['name (new)'] })
		expect(isNewColumn(p, 'name (new)')).toBe(true)
		expect(isNewColumn(p, 'name')).toBe(false)
	})
})

describe('the write dialog names the connection', () => {
	it('says which connection is production', () => {
		const prompt: WriteGuardPrompt = {
			code: 'writeGuard',
			kind: 'unlock',
			environment: 'production',
			connectionName: 'Dev server',
			preview: 'UPDATE t SET a = 1',
		}
		expect(guardCopy(prompt).description).toContain('“Dev server” is production.')
	})
})
