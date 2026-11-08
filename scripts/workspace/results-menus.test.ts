import assert from 'node:assert/strict'
import { test } from 'vitest'

import {
	resultCellMenu,
	resultCellPick,
	resultRowMenu,
	resultRowPick,
} from '../../src/lib/features/workspace/query/results-menus.ts'

test('every results menu item maps to an action', () => {
	for (const item of resultCellMenu(false, '⌘+C')) {
		assert.ok(resultCellPick(item.id), item.id)
	}
	for (const item of resultRowMenu('⌘+C')) {
		assert.ok(resultRowPick(item.id), item.id)
	}
})

test('row copies keep their format; cell copies stay cell copies', () => {
	assert.deepEqual(resultCellPick('copy-row-insert'), { kind: 'row', format: 'insert' })
	assert.deepEqual(resultCellPick('copy-row'), { kind: 'row', format: 'copy' })
	assert.deepEqual(resultCellPick('copy-tsv'), { kind: 'cells', format: 'tsv' })
	assert.equal(resultRowPick('copy-in'), 'in')
	assert.equal(resultCellPick('nope'), null)
})

test('View in modal is off for a multi-cell selection', () => {
	assert.equal(resultCellMenu(true, '')[0]?.disabled, true)
	assert.equal(resultCellMenu(false, '')[0]?.disabled, false)
})
