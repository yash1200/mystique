import assert from 'node:assert/strict'
import { test } from 'vitest'

import { cellDisplay } from '../../src/lib/features/workspace/data-grid/virtual.ts'
import {
	cellText,
} from '../../src/lib/features/workspace/data-grid/cell-value.ts'
import {
	FULL_VALUES_MAX_CHARS,
	isLargeCell,
	largeCellText,
	PARTIAL_COPY_MESSAGE,
	previewsNote,
	resolveLargeCells,
} from '../../src/lib/features/workspace/data-grid/large-cell.ts'

const marker = {
	$large: true as const,
	preview: 'aaaa…',
	totalChars: 200_000,
	locator: {
		table: 'docs',
		column: 'body',
		keys: [{ column: 'id', value: '9007199254740993' }],
	},
}

test('a large cell shows its preview, not the marker', () => {
	assert.equal(isLargeCell(marker), true)
	assert.equal(cellDisplay(marker), 'aaaa…')
	assert.equal(cellText(marker), 'aaaa…')
	assert.equal(largeCellText(marker), marker.preview)
	assert.equal(cellDisplay('short'), 'short')
	assert.equal(cellDisplay(null), 'NULL')
	assert.ok(PARTIAL_COPY_MESSAGE.includes('preview'))
})

test('a cell without a key is still large and not fetchable', () => {
	const unkeyed = { $large: true as const, preview: 'x…', totalChars: 40_000 }
	assert.equal(isLargeCell(unkeyed), true)
	assert.equal(unkeyed.locator, undefined)
})

test('copy and export load keyed values in full and count the previews', async () => {
	const full = 'a'.repeat(70_000) + 'END'
	const calls: number[] = []
	const load = async (_locator: unknown, offset: number) => {
		calls.push(offset)
		const end = Math.min(offset + 32_000, full.length)
		return {
			text: full.slice(offset, end),
			offset,
			nextOffset: end < full.length ? end : null,
			totalChars: full.length,
		}
	}
	const unkeyed = { $large: true as const, preview: 'x…', totalChars: 40_000 }
	const rows = [
		[1, { ...marker, totalChars: full.length }, unkeyed],
		[2, 'short', null],
	]
	const resolved = await resolveLargeCells(rows, load)
	assert.equal(resolved.cells[0][1], full)
	assert.deepEqual(calls, [0, 32_000, 64_000], 'one read per chunk, in order')
	assert.equal(resolved.cells[0][2], unkeyed, 'no key: left as a preview')
	assert.equal(resolved.previews, 1)
	assert.ok(previewsNote(1).includes('preview'))
	assert.equal(rows[0][1].preview, 'aaaa…', 'the grid rows are not mutated')

	// JSON columns come back as JSON.
	const json = await resolveLargeCells(
		[[{ ...marker, totalChars: 9 }]],
		async () => ({ text: '{"a":[1]}', offset: 0, nextOffset: null, totalChars: 9 }),
		[true],
	)
	assert.deepEqual(json.cells[0][0], { a: [1] })

	// Without a loader nothing is fetched, and nothing is silently partial.
	const none = await resolveLargeCells(rows, null)
	assert.equal(none.previews, 2)
})

test('a block of values past the cap is refused before loading', async () => {
	let loaded = 0
	const huge = { ...marker, totalChars: FULL_VALUES_MAX_CHARS + 1 }
	await assert.rejects(
		resolveLargeCells([[huge]], async () => {
			loaded++
			return { text: '', offset: 0, nextOffset: null, totalChars: 0 }
		}),
		/more than/,
	)
	assert.equal(loaded, 0)
})
