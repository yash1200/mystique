import { expect, test, vi } from 'vitest'

// E-08: history rows say what wrote them (a run, a grid save, a structure
// change, a rollback, EXPLAIN ANALYZE).
vi.mock('#core/ipc', () => ({
	errorMessage: (e: unknown) => String(e),
	invoke: async () => null,
}))

const { historyItemToEntry, historyKindLabel } = await import(
	'../../src/lib/features/workspace/query/history/history-api'
)

const row = (kind: unknown) =>
	historyItemToEntry({
		id: '1',
		body: 'UPDATE t SET a = 1',
		success: true,
		ranAt: '2026-11-09T00:00:00Z',
		kind: kind as never,
	})

test('an action row keeps its kind and gets a label', () => {
	expect(row('grid_save').kind).toBe('grid_save')
	expect(historyKindLabel('grid_save')).toBe('Grid save')
	expect(historyKindLabel('ddl')).toBe('Structure')
	expect(historyKindLabel('rollback')).toBe('Rollback')
	expect(historyKindLabel('explain_analyze')).toBe('Explain analyze')
})

test('a query run, an old row and an unknown kind read as a plain query', () => {
	expect(row('query').kind).toBe('query')
	expect(row(undefined).kind).toBe('query')
	expect(row('something-new').kind).toBe('query')
	expect(historyKindLabel('query')).toBeNull()
})
