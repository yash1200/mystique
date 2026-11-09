/**
 * Query editor: large-document mode, statement navigation, and catalog
 * completions that wait for the catalog but never answer late.
 */
import type { CompletionContext, CompletionSource } from '@codemirror/autocomplete'
import { EditorState } from '@codemirror/state'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { CatalogColumnRow } from '../../src/lib/core/contracts'
import { catalogColumnsCache } from '../../src/lib/features/workspace/catalog/columns.svelte'
import {
	gatedCompletion,
	isStale,
	type CatalogFeed,
} from '../../src/lib/features/workspace/query/editor/completion-gate'
import {
	isLargeDocument,
	LARGE_DOC_CHARS,
	LARGE_DOC_LINES,
} from '../../src/lib/features/workspace/query/editor/large-doc'
import {
	neighbourStatement,
	splitStatementSpans,
	statementAt,
} from '../../src/lib/features/workspace/query/editor/statement-span'
import { keymapHarness, type Harness } from './keymap-harness'
import { mockIpc } from './mock-ipc'

const row = (table: string, column: string): CatalogColumnRow =>
	({ schema: 'public', table, column, dataType: 'int', pk: false, nullable: true }) as CatalogColumnRow

describe('large-document mode', () => {
	it('starts above either limit', () => {
		expect(isLargeDocument(LARGE_DOC_CHARS, 10, false)).toBe(false)
		expect(isLargeDocument(LARGE_DOC_CHARS + 1, 10, false)).toBe(true)
		expect(isLargeDocument(100, LARGE_DOC_LINES + 1, false)).toBe(true)
	})

	it('stays on until the document is clearly smaller', () => {
		const near = Math.floor(LARGE_DOC_CHARS * 0.95)
		expect(isLargeDocument(near, 10, true)).toBe(true)
		expect(isLargeDocument(near, 10, false)).toBe(false)
		expect(isLargeDocument(Math.floor(LARGE_DOC_CHARS * 0.8), 10, true)).toBe(false)
	})
})

describe('statement navigation', () => {
	const sql = 'SELECT 1;\n\nSELECT 2;\nSELECT 3;'
	const spans = splitStatementSpans(sql, 'postgres')
	const at = (text: string) => sql.indexOf(text)

	it('goes to the next statement start', () => {
		expect(neighbourStatement(spans, at('SELECT 1') + 3, 1)?.text).toBe('SELECT 2')
		expect(neighbourStatement(spans, at('SELECT 3'), 1)).toBeNull()
	})

	it('goes back to this statement start first, then the previous one', () => {
		expect(neighbourStatement(spans, at('SELECT 2') + 4, -1)?.text).toBe('SELECT 2')
		expect(neighbourStatement(spans, at('SELECT 2'), -1)?.text).toBe('SELECT 1')
		expect(neighbourStatement(spans, 0, -1)).toBeNull()
	})

	it('selects the statement whose region holds the caret', () => {
		// The blank line after a statement still belongs to it.
		expect(statementAt(spans, at('SELECT 2') - 1)?.text).toBe('SELECT 1')
		expect(statementAt(spans, at('SELECT 2') + 2)?.text).toBe('SELECT 2')
	})
})

describe('statement keys', () => {
	const harnesses: Harness[] = []
	afterEach(() => {
		for (const h of harnesses.splice(0)) h.dispose()
	})

	it('uses ctrl+alt arrows on every platform and leaves cmd+alt arrows to the editor', () => {
		for (const onMac of [true, false]) {
			// One harness at a time: both would answer the same window keydown.
			const h = keymapHarness({ mac: onMac })
			expect(h.press({ ctrlKey: true, altKey: true, key: 'ArrowDown' })).toEqual([
				'query.next-statement',
			])
			expect(h.press({ ctrlKey: true, altKey: true, key: 'ArrowUp' })).toEqual([
				'query.prev-statement',
			])
			h.dispose()
		}
		const mac = keymapHarness({ mac: true })
		harnesses.push(mac)
		expect(mac.press({ metaKey: true, altKey: true, key: 'ArrowDown' })).toEqual([])
		expect(mac.press({ metaKey: true, shiftKey: true, key: ' ', code: 'Space' })).toEqual([
			'query.select-statement',
		])
	})
})

describe('catalog completion gate', () => {
	function context(over: Partial<CompletionContext> = {}): CompletionContext {
		const state = EditorState.create({ doc: 'SELECT * FROM ' })
		return { state, pos: 14, aborted: false, view: undefined, ...over } as CompletionContext
	}
	const answer: CompletionSource = () => ({ from: 14, options: [{ label: 'x' }] })

	it('answers from the built rows when the catalog is not loading', async () => {
		const direct = vi.fn(answer)
		const make = vi.fn(() => answer)
		const source = gatedCompletion(direct, make, () => ({ key: 'k', rows: [], pending: null }))
		await source(context())
		expect(direct).toHaveBeenCalledOnce()
		expect(make).not.toHaveBeenCalled()
	})

	it('waits for the load in flight and answers from its rows', async () => {
		let finish: (rows: CatalogColumnRow[]) => void = () => {}
		const pending = new Promise<CatalogColumnRow[]>((resolve) => (finish = resolve))
		const make = vi.fn(() => answer)
		const source = gatedCompletion(answer, make, () => ({ key: 'k', rows: [], pending }))
		const result = source(context())
		finish([row('orders', 'id')])
		expect(await result).toMatchObject({ options: [{ label: 'x' }] })
		expect(make).toHaveBeenCalledWith([row('orders', 'id')])
	})

	it('drops the answer when the database changed while it waited', async () => {
		let feed: CatalogFeed = { key: 'db1', rows: [], pending: Promise.resolve([row('a', 'b')]) }
		const source = gatedCompletion(answer, () => answer, () => feed)
		const result = source(context())
		feed = { key: 'db2', rows: [], pending: null }
		expect(await result).toBeNull()
	})

	it('drops the answer when the request was aborted or the document moved on', () => {
		const feed = () => ({ key: 'k', rows: [], pending: null })
		expect(isStale(context({ aborted: true }), 'k', feed)).toBe(true)
		const ctx = context()
		const moved = { state: EditorState.create({ doc: 'SELECT * FROM o' }) }
		expect(isStale({ ...ctx, view: moved } as unknown as CompletionContext, 'k', feed)).toBe(true)
		// Same document, caret elsewhere.
		const same = { state: ctx.state }
		expect(isStale({ ...ctx, view: same } as unknown as CompletionContext, 'k', feed)).toBe(
			true,
		)
		// A selection change keeps the document; the caret is where it asked.
		const here = { state: ctx.state.update({ selection: { anchor: 14 } }).state }
		expect(here.state.doc).toBe(ctx.state.doc)
		expect(isStale({ ...ctx, view: here } as unknown as CompletionContext, 'k', feed)).toBe(
			false,
		)
	})

	it('gives up after the wait limit', async () => {
		vi.useFakeTimers()
		try {
			const never = new Promise<CatalogColumnRow[]>(() => {})
			const source = gatedCompletion(answer, () => answer, () => ({
				key: 'k',
				rows: [],
				pending: never,
			}), 50)
			const result = source(context())
			await vi.advanceTimersByTimeAsync(60)
			expect(await result).toBeNull()
		} finally {
			vi.useRealTimers()
		}
	})
})

describe('catalog columns cache', () => {
	afterEach(() => catalogColumnsCache.invalidate())

	it('exposes the load in flight and drops it after an invalidate', async () => {
		let finish: (rows: CatalogColumnRow[]) => void = () => {}
		mockIpc({
			list_engine_catalog_columns: () =>
				new Promise<CatalogColumnRow[]>((resolve) => (finish = resolve)),
		})
		const load = catalogColumnsCache.load('s1', 'app')
		expect(catalogColumnsCache.pending('s1', 'app')).toBeInstanceOf(Promise)
		await Promise.resolve()
		catalogColumnsCache.invalidate('s1')
		expect(catalogColumnsCache.pending('s1', 'app')).toBeNull()
		finish([row('orders', 'id')])
		await load
		expect(catalogColumnsCache.peek('s1', 'app')).toBeUndefined()
	})

	it('keeps the rows of a load nobody invalidated', async () => {
		mockIpc({ list_engine_catalog_columns: () => [row('orders', 'id')] })
		await catalogColumnsCache.load('s2', 'app')
		expect(catalogColumnsCache.peek('s2', 'app')).toEqual([row('orders', 'id')])
		expect(catalogColumnsCache.pending('s2', 'app')).toBeNull()
	})
})
