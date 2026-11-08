import { beforeEach, describe, expect, it, vi } from 'vitest'

const cancelled: string[] = []

vi.mock('../../src/lib/core/ipc/engines', () => ({
	cancelEngineQuery: vi.fn(async (id: string) => {
		cancelled.push(id)
	}),
}))

const { cancelTabRun, finishTabRun, startTabRun } = await import(
	'../../src/lib/core/ipc/runs'
)

describe('run slots', () => {
	beforeEach(() => {
		cancelled.length = 0
		;(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}
	})

	it('gives preview and export on one tab their own slot', async () => {
		const preview = startTabRun('t1', 'preview')
		const exported = startTabRun('t1', 'export')
		expect(preview).not.toBe(exported)
		expect(cancelled).toEqual([])

		await cancelTabRun('t1', 'preview')
		expect(cancelled).toEqual([preview])

		await cancelTabRun('t1', 'export')
		expect(cancelled).toEqual([preview, exported])
	})

	it('a new run of the same kind cancels the previous one', async () => {
		const first = startTabRun('t2')
		const second = startTabRun('t2')
		await Promise.resolve()
		expect(cancelled).toEqual([first])
		finishTabRun('t2', second)
		await cancelTabRun('t2')
		expect(cancelled).toEqual([first])
	})

	it('closing a tab cancels every run on it', async () => {
		const preview = startTabRun('t3', 'preview')
		const exported = startTabRun('t3', 'export')
		await cancelTabRun('t3')
		expect(cancelled.sort()).toEqual([preview, exported].sort())
	})
})
