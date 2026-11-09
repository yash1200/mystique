import { beforeEach, describe, expect, test, vi } from 'vitest'

// D-06: catalog state is keyed by connection. A refresh, a busy create or an
// error on one connection never shows on another.
const toasts: string[] = []
const session = { activeConnectionId: 'a' as string | null }

vi.mock('#features/toast', () => ({
	showToast: (_tone: string, message: string) => toasts.push(message),
}))
vi.mock('#features/workspace/session', async () => {
	const { selectedDatabases } = await import(
		'../../src/lib/features/workspace/session/selected-db.svelte'
	)
	const { selectedDatabaseName } = await import(
		'../../src/lib/features/workspace/session/lookup'
	)
	return { selectedDatabases, selectedDatabaseName, workspaceSession: session }
})

const { catalogStore, currentDatabaseName, databaseNameFor } = await import(
	'../../src/lib/features/workspace/catalog/store.svelte'
)
const { selectedDatabases } = await import(
	'../../src/lib/features/workspace/session/selected-db.svelte'
)

const db = (name: string) => ({ name, meta: '', tables: [] })

beforeEach(() => {
	toasts.length = 0
	session.activeConnectionId = 'a'
	for (const id of ['a', 'b']) {
		catalogStore.forget(id)
		selectedDatabases.forget(id)
	}
})

describe('catalog store', () => {
	test('busy flags belong to one connection', () => {
		catalogStore.patchStatus('a', { refreshing: true, createBusy: true })
		expect(catalogStore.statusOf('a')).toMatchObject({
			refreshing: true,
			createBusy: true,
			refreshingTables: false,
		})
		expect(catalogStore.statusOf('b')).toMatchObject({
			refreshing: false,
			createBusy: false,
		})
		catalogStore.patchStatus('b', { refreshing: false })
		expect(catalogStore.statusOf('a').refreshing).toBe(true)
	})

	test('an error shows on its own connection and toasts once', () => {
		catalogStore.setError('a', 'boom')
		expect(catalogStore.statusOf('a').error).toBe('boom')
		expect(catalogStore.statusOf('b').error).toBeNull()
		expect(toasts).toEqual(['boom'])
		catalogStore.setError('a', null)
		expect(catalogStore.statusOf('a').error).toBeNull()
		expect(toasts).toEqual(['boom'])
	})

	test('no connection reads as idle and empty', () => {
		expect(catalogStore.statusOf(null).refreshing).toBe(false)
		expect(catalogStore.databases(null)).toEqual([])
		expect(databaseNameFor(null)).toBe('')
	})

	test('forget drops one connection only', () => {
		catalogStore.setDatabases('a', [db('app')])
		catalogStore.setDatabases('b', [db('shop')])
		catalogStore.liveKey = { a: 's1', b: 's2' }
		catalogStore.patchStatus('a', { error: 'x' })
		catalogStore.forget('a')
		expect(catalogStore.databases('a')).toEqual([])
		expect(catalogStore.liveKey).toEqual({ b: 's2' })
		expect(catalogStore.statusOf('a').error).toBeNull()
		expect(catalogStore.databases('b').map((d) => d.name)).toEqual(['shop'])
	})

	test('invalidate forces the next load for that connection', () => {
		catalogStore.liveKey = { a: 's1', b: 's2' }
		catalogStore.invalidate('a')
		expect(catalogStore.liveKey).toEqual({ a: '', b: 's2' })
	})

	test('the working database is per connection', () => {
		catalogStore.setDatabases('a', [db('app'), db('logs')])
		catalogStore.setDatabases('b', [db('shop')])
		expect(databaseNameFor('a')).toBe('app')
		expect(databaseNameFor('b')).toBe('shop')
		selectedDatabases.set('a', 'logs')
		expect(databaseNameFor('a')).toBe('logs')
		expect(databaseNameFor('b')).toBe('shop')
		expect(currentDatabaseName()).toBe('logs')
		session.activeConnectionId = 'b'
		expect(currentDatabaseName()).toBe('shop')
	})
})
