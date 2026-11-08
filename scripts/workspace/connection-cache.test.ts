import { describe, expect, test, vi } from 'vitest'
import { ConnectionCache } from '../../src/lib/features/workspace/session/connection-cache.svelte'

vi.mock('#features/workspace/session', () =>
	import('../../src/lib/features/workspace/session/connection-cache.svelte'),
)

const { modelKey } = await import('../../src/lib/features/workspace/er/store.svelte')

// D-06: structures and ER models are cached per connection, and so are their
// loading flag and error. Before, one global error blocked hydrate everywhere.
describe('ConnectionCache', () => {
	test('entries with the same key stay apart per connection', () => {
		const cache = new ConnectionCache<number>()
		cache.put('a', 'users', 1)
		cache.put('b', 'users', 2)
		expect(cache.get('a', 'users')).toBe(1)
		expect(cache.get('b', 'users')).toBe(2)
		expect(cache.get('c', 'users')).toBeUndefined()
		expect(cache.get('a', null)).toBeUndefined()
	})

	test('loading and error are per connection', () => {
		const cache = new ConnectionCache<number>()
		cache.setLoading('a', true)
		cache.setError('a', 'no such table')
		expect(cache.of('a')).toMatchObject({ loading: true, error: 'no such table' })
		expect(cache.of('b')).toMatchObject({ loading: false, error: null })
		expect(cache.of(null)).toMatchObject({ loading: false, error: null })
	})

	test('flags and entries do not clobber each other', () => {
		const cache = new ConnectionCache<number>()
		cache.put('a', 'x', 1)
		cache.setLoading('a', true)
		cache.setError('a', 'e')
		cache.put('a', 'y', 2)
		expect(cache.of('a')).toMatchObject({
			entries: { x: 1, y: 2 },
			loading: true,
			error: 'e',
		})
	})

	test('drop and forget touch only their target', () => {
		const cache = new ConnectionCache<number>()
		cache.put('a', 'x', 1)
		cache.put('a', 'y', 2)
		cache.put('b', 'x', 3)
		cache.drop('a', 'x')
		expect(cache.of('a').entries).toEqual({ y: 2 })
		cache.forget('a')
		expect(cache.of('a').entries).toEqual({})
		expect(cache.get('b', 'x')).toBe(3)
	})

	test('the empty slot is never shared state', () => {
		const cache = new ConnectionCache<number>()
		const empty = cache.of('a')
		cache.put('a', 'x', 1)
		expect(empty.entries).toEqual({})
		expect(cache.of('z').entries).toEqual({})
	})
})

test('ER model keys are database + table inside a connection', () => {
	expect(modelKey(' app ', 'public.users')).toBe('app::public.users')
	expect(modelKey('', null)).toBe('_::_')
})
