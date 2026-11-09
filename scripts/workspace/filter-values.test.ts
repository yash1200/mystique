import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

import {
	codeValueContext,
	insertCodeValue,
	matchValues,
} from '../../src/lib/features/workspace/table/filters/value-complete'
import {
	VALUE_DEBOUNCE_MS,
	VALUE_PREFIX_LIMIT,
	ValueSuggester,
} from '../../src/lib/features/workspace/table/filters/value-suggest.svelte'

const columns = ['id', 'status', 'email']

describe('code filter value context', () => {
	test('a quoted or bare value after a known column', () => {
		const code = "id > 3 AND status = 'pen"
		expect(codeValueContext(code, code.length, columns)).toEqual({
			column: 'status',
			start: code.indexOf("'"),
			end: code.length,
			prefix: 'pen',
		})
		const bare = 'email starts with ad'
		expect(codeValueContext(bare, bare.length, columns)).toMatchObject({
			column: 'email',
			prefix: 'ad',
			start: bare.length - 2,
		})
		expect(codeValueContext('STATUS = ', 9, columns)?.column).toBe('status')
	})

	test('not a value position, or an unknown column', () => {
		expect(codeValueContext('stat', 4, columns)).toBeNull()
		expect(codeValueContext("nope = 'a", 9, columns)).toBeNull()
		expect(codeValueContext("status = 'done' ", 16, columns)).toBeNull()
	})

	test('a pick replaces the typed value and its closing quote', () => {
		const code = "status = 'pe' AND id = 1"
		const ctx = codeValueContext(code, 12, columns)
		expect(ctx).not.toBeNull()
		const next = insertCodeValue(code, ctx!, 'pending')
		expect(next.code).toBe("status = 'pending' AND id = 1")
		expect(next.caret).toBe("status = 'pending'".length)
	})

	test('matching is a case-insensitive prefix, bounded', () => {
		expect(matchValues(['Alpha', 'alps', 'beta', 'al'], 'al')).toEqual(['Alpha', 'alps'])
		expect(matchValues(['x'.repeat(300), 'line\nbreak', 'ok'], '')).toEqual(['ok'])
		expect(matchValues(Array.from({ length: 50 }, (_, i) => `v${i}`), 'v', 5)).toHaveLength(5)
	})
})

describe('value suggester', () => {
	beforeEach(() => vi.useFakeTimers())
	afterEach(() => vi.useRealTimers())

	function make(fetch: (column: string, prefix: string, limit: number) => Promise<string[]>) {
		return new ValueSuggester({ fetch, enabled: () => true, scope: () => 'sid\nt' })
	}

	test('debounced; a complete list answers later prefixes locally', async () => {
		const fetch = vi.fn(async () => ['open', 'pending', 'paid'])
		const s = make(fetch)
		s.request('status', '')
		s.request('status', 'p')
		expect(fetch).not.toHaveBeenCalled()
		await vi.advanceTimersByTimeAsync(VALUE_DEBOUNCE_MS)
		expect(fetch).toHaveBeenCalledTimes(1)
		expect(fetch).toHaveBeenCalledWith('status', '', 1000)
		expect(s.items).toEqual(['pending', 'paid'])
		s.request('status', 'pa')
		expect(s.items).toEqual(['paid'])
		await vi.advanceTimersByTimeAsync(VALUE_DEBOUNCE_MS)
		expect(fetch).toHaveBeenCalledTimes(1)
	})

	test('a wide column asks the server per prefix', async () => {
		const many = Array.from({ length: 1000 }, (_, i) => `user${i}@x`)
		const fetch = vi.fn(async (_c: string, prefix: string) =>
			prefix ? ['user9999@x', 'user99999@x'] : many,
		)
		const s = make(fetch)
		s.request('email', 'user9999')
		await vi.advanceTimersByTimeAsync(VALUE_DEBOUNCE_MS)
		expect(fetch).toHaveBeenLastCalledWith('email', 'user9999', VALUE_PREFIX_LIMIT)
		expect(s.items).toEqual(['user9999@x', 'user99999@x'])
		// The shorter answer was complete, so a longer prefix needs no request.
		s.request('email', 'user99999')
		expect(s.items).toEqual(['user99999@x'])
		expect(fetch).toHaveBeenCalledTimes(2)
	})

	test('a stale answer is dropped', async () => {
		let release: (v: string[]) => void = () => {}
		const fetch = vi
			.fn<(c: string, p: string, l: number) => Promise<string[]>>()
			.mockImplementationOnce(() => new Promise((resolve) => (release = resolve)))
			.mockImplementation(async () => ['b1', 'b2'])
		const s = make(fetch)
		s.request('a', '')
		await vi.advanceTimersByTimeAsync(VALUE_DEBOUNCE_MS)
		s.request('b', '')
		await vi.advanceTimersByTimeAsync(VALUE_DEBOUNCE_MS)
		expect(s.items).toEqual(['b1', 'b2'])
		release(['a1'])
		await vi.advanceTimersByTimeAsync(0)
		expect(s.items).toEqual(['b1', 'b2'])
	})
})
