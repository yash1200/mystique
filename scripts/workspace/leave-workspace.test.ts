import { describe, expect, it } from 'vitest'
import {
	hasPendingLoss,
	workspaceLoss,
} from '../../src/lib/features/workspace/session/close-guard'

const tabs = [
	{ id: 'q1', kind: 'query' as const, title: 'Query 1', dirty: false },
	{ id: 't1', kind: 'table' as const, title: 'users', dirty: false },
	{ id: 'q2', kind: 'query' as const, title: 'Query 2', dirty: false },
]

describe('leave workspace', () => {
	it('leaves at once when nothing is pending', () => {
		const loss = workspaceLoss(
			tabs,
			{ q1: { pinned: true, inTransaction: false } },
			() => false,
		)
		expect(hasPendingLoss(loss)).toBe(false)
	})

	it('asks when a grid has unsaved edits', () => {
		const loss = workspaceLoss(tabs, {}, (id) => id === 't1')
		expect(loss).toEqual({ dirty: ['users'], transactions: [] })
	})

	it('ignores grid edits reported for a non-table tab', () => {
		const loss = workspaceLoss(tabs, {}, (id) => id === 'q1')
		expect(hasPendingLoss(loss)).toBe(false)
	})

	it('asks when a transaction is open', () => {
		const loss = workspaceLoss(
			tabs,
			{ q2: { pinned: true, inTransaction: true } },
			() => false,
		)
		expect(loss).toEqual({ dirty: [], transactions: ['Query 2'] })
	})
})
