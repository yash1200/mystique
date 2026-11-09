import { beforeEach, describe, expect, test, vi } from 'vitest'

// D-06: history and saved-query lists are keyed by connection. A late answer
// for one connection lands in that connection's list, never on screen for
// another.
type Deferred<T> = { promise: Promise<T>; resolve: (v: T) => void }
function deferred<T>(): Deferred<T> {
	let resolve!: (v: T) => void
	const promise = new Promise<T>((r) => (resolve = r))
	return { promise, resolve }
}

const session = {
	activeConnectionId: 'a' as string | null,
	sessions: [] as unknown[],
	openNewQueryWithText: () => {},
	openSavedQuery: () => {},
}
const savedCalls: { id: string; d: Deferred<unknown[]> }[] = []
const historyCalls: { id: string; d: Deferred<unknown> }[] = []

vi.mock('#core/ipc', () => ({
	isTauri: () => true,
	errorMessage: (e: unknown) => String(e),
}))
vi.mock('#engines', () => ({}))
vi.mock('#features/workspace/session', () => ({ workspaceSession: session }))
vi.mock('../../src/lib/features/workspace/query/policy', () => ({}))
vi.mock('../../src/lib/features/workspace/query/saved/saved-api', () => ({
	listSavedQueries: (id: string) => {
		const d = deferred<unknown[]>()
		savedCalls.push({ id, d })
		return d.promise
	},
	toListSavedQuery: (q: { id: string; connectionId: string }) => ({
		id: q.id,
		connectionId: q.connectionId,
		name: q.id,
		sql: '',
	}),
}))
vi.mock('../../src/lib/features/workspace/query/history/history-api', () => ({
	listQueryHistory: (id: string) => {
		const d = deferred<unknown>()
		historyCalls.push({ id, d })
		return d.promise
	},
	listRecentQueryHistory: async () => ({ items: [], hasMore: false }),
	historyItemToEntry: (it: { id: string; connectionId: string }) => ({
		id: it.id,
		sql: it.id,
		connectionId: it.connectionId,
		ranAt: '',
	}),
}))

const { refreshSavedQueries, savedStore } = await import(
	'../../src/lib/features/workspace/query/saved/library.svelte'
)
const { historyStore, loadOlderHistory, prependHistoryItem, refreshHistory } =
	await import('../../src/lib/features/workspace/query/history/history.svelte')

const flush = () => new Promise((r) => setTimeout(r, 0))
const row = (id: string, connectionId: string) => ({ id, connectionId })

beforeEach(() => {
	savedCalls.length = 0
	historyCalls.length = 0
	session.activeConnectionId = 'a'
	for (const id of ['a', 'b']) {
		savedStore.forget(id)
		historyStore.forget(id)
	}
})

describe('saved queries', () => {
	test('a late answer for the previous connection stays on that connection', async () => {
		void refreshSavedQueries('a', true)
		session.activeConnectionId = 'b'
		void refreshSavedQueries('b', true)
		savedCalls[1]!.d.resolve([row('q-b', 'b')])
		await flush()
		savedCalls[0]!.d.resolve([row('q-a', 'a')])
		await flush()
		expect(savedStore.current.items.map((q) => q.id)).toEqual(['q-b'])
		expect(savedStore.of('a').items.map((q) => q.id)).toEqual(['q-a'])
	})

	test('a superseded fetch for the same connection is dropped', async () => {
		void refreshSavedQueries('a', true)
		void refreshSavedQueries('a', true)
		savedCalls[1]!.d.resolve([row('new', 'a')])
		await flush()
		savedCalls[0]!.d.resolve([row('old', 'a')])
		await flush()
		expect(savedStore.of('a').items.map((q) => q.id)).toEqual(['new'])
	})

	test('selection and forget are per connection', () => {
		savedStore.select('a', 'q1')
		savedStore.select('b', 'q2')
		expect(savedStore.of('a').selectedId).toBe('q1')
		savedStore.forget('a')
		expect(savedStore.of('a').selectedId).toBeNull()
		expect(savedStore.of('b').selectedId).toBe('q2')
	})
})

describe('query history', () => {
	test('each connection keeps its own list and loading flag', async () => {
		void refreshHistory({ force: true })
		expect(historyStore.of('a').loading).toBe(true)
		session.activeConnectionId = 'b'
		void refreshHistory({ force: true })
		expect(historyStore.of('b').loading).toBe(true)
		historyCalls[0]!.d.resolve({ items: [row('h-a', 'a')], hasMore: false })
		historyCalls[1]!.d.resolve({ items: [row('h-b', 'b')], hasMore: true })
		await flush()
		expect(historyStore.current.entries.map((h) => h.id)).toEqual(['h-b'])
		expect(historyStore.of('a').entries.map((h) => h.id)).toEqual(['h-a'])
		expect(historyStore.of('a').loading).toBe(false)
		expect(historyStore.of('b').hasMore).toBe(true)
	})

	test('a run on another connection is not added to the active list', () => {
		session.activeConnectionId = 'b'
		prependHistoryItem({ id: 'x', connectionId: 'a' } as never)
		expect(historyStore.of('b').entries).toEqual([])
		prependHistoryItem({ id: 'y', connectionId: 'b' } as never)
		expect(historyStore.of('b').entries.map((h) => h.id)).toEqual(['y'])
		expect(historyStore.of('b').selectedId).toBe('y')
	})

	test('load more on one connection does not block another', async () => {
		historyStore.patch('a', {
			entries: [{ id: 'h1', ranAt: '' }] as never,
			hasMore: true,
			loaded: true,
		})
		void loadOlderHistory()
		expect(historyStore.of('a').loadingMore).toBe(true)
		expect(historyStore.of('b').loadingMore).toBe(false)
		// A refresh lands first; the older page is dropped and the flag clears.
		void refreshHistory({ force: true })
		historyCalls[1]!.d.resolve({ items: [row('h2', 'a')], hasMore: false })
		await flush()
		historyCalls[0]!.d.resolve({ items: [row('h0', 'a')], hasMore: false })
		await flush()
		expect(historyStore.of('a').entries.map((h) => h.id)).toEqual(['h2'])
		expect(historyStore.of('a').loadingMore).toBe(false)
	})

	test('forget drops one connection only', () => {
		historyStore.patch('a', { entries: [{ id: 'h1' }] as never })
		historyStore.patch('b', { entries: [{ id: 'h2' }] as never })
		historyStore.forget('a')
		expect(historyStore.of('a').entries).toEqual([])
		expect(historyStore.of('b').entries.map((h) => h.id)).toEqual(['h2'])
	})
})
