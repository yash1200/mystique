import assert from 'node:assert/strict'
import { test } from 'vitest'

import { tableSync } from '../../src/lib/features/workspace/table/sync.svelte.ts'

test('table signals are per tab and per kind', () => {
	const a = 'conn::table-a'
	const b = 'conn::table-b'
	const before = tableSync.previewVersion(a)
	tableSync.previewLoaded(a)
	tableSync.previewLoaded(a)
	assert.equal(tableSync.previewVersion(a), before + 2)
	assert.equal(tableSync.previewVersion(b), 0)
	assert.equal(tableSync.rowsVersion(a), 0)

	tableSync.rowsEdited(b)
	tableSync.baselineChanged(b)
	assert.equal(tableSync.rowsVersion(b), 1)
	assert.equal(tableSync.baselineVersion(b), 1)
	assert.equal(tableSync.baselineVersion(a), 0)
})

test('a grid without a tab id sees no signals', () => {
	assert.equal(tableSync.rowsVersion(null), 0)
	assert.equal(tableSync.previewVersion(undefined), 0)
})
