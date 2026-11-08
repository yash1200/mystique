import assert from 'node:assert/strict'
import { test } from 'vitest'

import {
	CLONE_LARGE_REFUSED,
	cloneRowValues,
	type FetchChunk,
} from '../../src/lib/features/workspace/table/clone-row.ts'
import type { GridColumnLayout } from '../../src/lib/features/workspace/data-grid/column-layout.ts'
import {
	columnRange,
	columnsFrom,
	inVisibleOrder,
	stepColumn,
	visibleColumns,
} from '../../src/lib/features/workspace/data-grid/visible-columns.ts'

const columns = ['id', 'a', 'b', 'c', 'd'].map((name) => ({ name }))

function layout(part: Partial<GridColumnLayout>): GridColumnLayout {
	return { order: [], hidden: [], frozen: [], ...part }
}

test('R-22 visible columns drop hidden ones and keep table order otherwise', () => {
	const view = visibleColumns(columns, layout({ hidden: ['b'] }))
	assert.deepEqual(view.names, ['id', 'a', 'c', 'd'])
	assert.equal(view.frozen, 0)
})

test('R-22 reordered and frozen columns are in the order the grid draws them', () => {
	const view = visibleColumns(
		columns,
		layout({ order: ['d', 'c', 'b', 'a', 'id'], frozen: ['a'] }),
	)
	assert.deepEqual(view.names, ['a', 'd', 'c', 'b', 'id'])
	assert.equal(view.frozen, 1)
})

test('R-22 a shift-click rectangle spans visible columns, not table positions', () => {
	const view = visibleColumns(columns, layout({ order: ['d', 'a', 'b', 'c', 'id'], hidden: ['b'] }))
	assert.deepEqual(columnRange(view, 'd', 'c'), ['d', 'a', 'c'])
	assert.deepEqual(columnRange(view, 'c', 'd'), ['d', 'a', 'c'])
})

test('R-22 paste across a hidden column skips it and never writes it', () => {
	const view = visibleColumns(columns, layout({ hidden: ['b'] }))
	assert.deepEqual(columnsFrom(view, 'a', 3), ['a', 'c', 'd'])
	assert.deepEqual(columnsFrom(view, 'c', 5), ['c', 'd'])
	assert.deepEqual(columnsFrom(view, 'b', 2), [])
})

test('R-22 paste starting in a frozen column continues into the scrolling ones', () => {
	const view = visibleColumns(columns, layout({ frozen: ['c'] }))
	assert.deepEqual(columnsFrom(view, 'c', 3), ['c', 'id', 'a'])
})

test('R-22 arrow keys step through visible columns and clamp at the ends', () => {
	const view = visibleColumns(columns, layout({ hidden: ['b'], frozen: ['d'] }))
	assert.deepEqual(view.names, ['d', 'id', 'a', 'c'])
	assert.equal(stepColumn(view, 'a', 1), 'c')
	assert.equal(stepColumn(view, 'c', 1), 'c')
	assert.equal(stepColumn(view, 'id', -1), 'd')
	assert.equal(stepColumn(view, 'd', -1), 'd')
	assert.equal(stepColumn(view, null, 0), 'd')
	assert.equal(stepColumn(visibleColumns([], null), 'a', 1), undefined)
})

test('R-22 copied cells come out in visible order without hidden columns', () => {
	const view = visibleColumns(columns, layout({ order: ['c', 'a'], hidden: ['d'] }))
	assert.deepEqual(inVisibleOrder(view, ['a', 'd', 'c', 'id']), ['c', 'a', 'id'])
})

const bodyLocator = {
	table: 'docs',
	column: 'body',
	keys: [{ column: 'id', value: 7 }],
}

test('R-31 duplicating a row loads the full large value instead of the marker', async () => {
	const calls: number[] = []
	const fetchChunk: FetchChunk = async (_locator, offset) => {
		calls.push(offset)
		return offset === 0
			? { text: 'abc', offset: 0, nextOffset: 3, totalChars: 5 }
			: { text: 'de', offset: 3, nextOffset: null, totalChars: 5 }
	}
	const clone = await cloneRowValues({
		row: {
			id: 7,
			body: { $large: true, preview: 'ab…', totalChars: 5, locator: bodyLocator },
			doc: {
				$large: true,
				preview: '{"a"…',
				totalChars: 7,
				locator: { ...bodyLocator, column: 'doc' },
			},
		},
		columns: [
			{ name: 'id', type: 'int4', pk: true },
			{ name: 'body', type: 'text' },
			{ name: 'doc', type: 'jsonb' },
		],
		fetchChunk: async (locator, offset) =>
			locator.column === 'doc'
				? { text: '{"a":1}', offset: 0, nextOffset: null, totalChars: 7 }
				: fetchChunk(locator, offset),
	})
	assert.deepEqual(clone, { id: null, body: 'abcde', doc: { a: 1 } })
	assert.deepEqual(calls, [0, 3])
	assert.ok(!JSON.stringify(clone).includes('$large'))
})

test('R-31 duplicating a row with an unkeyed large value is refused', async () => {
	await assert.rejects(
		cloneRowValues({
			row: { name: 'x', body: { $large: true, preview: 'ab…', totalChars: 99 } },
			columns: [{ name: 'name' }, { name: 'body' }],
			fetchChunk: async () => ({ text: '', offset: 0, nextOffset: null, totalChars: 0 }),
		}),
		{ message: CLONE_LARGE_REFUSED },
	)
	await assert.rejects(
		cloneRowValues({
			row: { id: 1, body: { $large: true, preview: 'ab…', totalChars: 99, locator: bodyLocator } },
			columns: [{ name: 'id', pk: true }, { name: 'body' }],
			fetchChunk: null,
		}),
		{ message: CLONE_LARGE_REFUSED },
	)
})

test('R-31 a row without large values clones without fetching', async () => {
	const clone = await cloneRowValues({
		row: { id: 3, name: 'a' },
		columns: [{ name: 'id', pk: true }, { name: 'name' }],
		fetchChunk: async () => {
			throw new Error('should not fetch')
		},
	})
	assert.deepEqual(clone, { id: null, name: 'a' })
})
