import assert from 'node:assert/strict'
import { test } from 'vitest'

import { cellDisplay } from '../../src/lib/features/workspace/data-grid/virtual.ts'
import {
	cellText,
} from '../../src/lib/features/workspace/data-grid/cell-value.ts'
import {
	isLargeCell,
	largeCellText,
	PARTIAL_COPY_MESSAGE,
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
