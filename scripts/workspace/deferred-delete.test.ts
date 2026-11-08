import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import {
	DeferredDeletes,
	restoreAt,
} from '../../src/lib/features/workspace/agent/deferred-delete'

describe('DeferredDeletes', () => {
	beforeEach(() => vi.useFakeTimers())
	afterEach(() => vi.useRealTimers())

	it('deletes on the backend only after the undo window', () => {
		const committed: string[] = []
		const deletes = new DeferredDeletes((id) => void committed.push(id), 8000)
		deletes.schedule('c1')
		expect(deletes.has('c1')).toBe(true)
		vi.advanceTimersByTime(7999)
		expect(committed).toEqual([])
		vi.advanceTimersByTime(1)
		expect(committed).toEqual(['c1'])
		expect(deletes.has('c1')).toBe(false)
	})

	it('undo inside the window never reaches the backend', () => {
		const committed: string[] = []
		const deletes = new DeferredDeletes((id) => void committed.push(id), 8000)
		deletes.schedule('c1')
		expect(deletes.cancel('c1')).toBe(true)
		vi.advanceTimersByTime(10_000)
		expect(committed).toEqual([])
	})

	it('undo after the delete ran does nothing', () => {
		const deletes = new DeferredDeletes(() => {}, 8000)
		deletes.schedule('c1')
		vi.advanceTimersByTime(8000)
		expect(deletes.cancel('c1')).toBe(false)
	})

	it('flush commits everything pending once', async () => {
		const committed: string[] = []
		const deletes = new DeferredDeletes((id) => void committed.push(id), 8000)
		deletes.schedule('c1')
		deletes.schedule('c2')
		await deletes.flush()
		vi.advanceTimersByTime(10_000)
		expect(committed).toEqual(['c1', 'c2'])
	})
})

describe('restoreAt', () => {
	const a = { id: 'a' }
	const b = { id: 'b' }
	const c = { id: 'c' }

	it('puts the item back at its old index', () => {
		expect(restoreAt([a, c], b, 1)).toEqual([a, b, c])
		expect(restoreAt([a], c, 5)).toEqual([a, c])
	})

	it('does not duplicate an item already listed', () => {
		expect(restoreAt([a, b], b, 0)).toEqual([a, b])
	})
})
