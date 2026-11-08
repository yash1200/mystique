import assert from 'node:assert/strict'
import { test } from 'node:test'

import { exactNumberText, numericColumn, parseDraft } from '../src/lib/features/workspace/data-grid/cell-value.ts'

test('quoted int8 survives JSON.parse and an unquoted one does not', () => {
	const quoted = JSON.parse('{"id":"9007199254740993"}') as { id: unknown }
	assert.equal(quoted.id, '9007199254740993')
	assert.equal(typeof quoted.id, 'string')

	const rounded = JSON.parse('{"id":9007199254740993}') as { id: number }
	assert.notEqual(String(rounded.id), '9007199254740993')
	assert.equal(rounded.id, 9007199254740992)
})

test('parseDraft keeps the exact int8 and numeric text', () => {
	assert.equal(
		parseDraft('9007199254740993', '9007199254740993'),
		'9007199254740993',
	)
	assert.equal(
		parseDraft('9223372036854775807', '1'),
		'9223372036854775807',
	)
	assert.equal(
		parseDraft('12345678901234567890.25', '0.25'),
		'12345678901234567890.25',
	)
	assert.equal(parseDraft('42', 7), 42)
	assert.equal(exactNumberText('9007199254740991'), 9007199254740991)
})

test('numeric column tag is not ordinary text or an interval', () => {
	assert.equal(numericColumn('int8'), true)
	assert.equal(numericColumn('numeric(20,2)'), true)
	assert.equal(numericColumn('bigint'), true)
	assert.equal(numericColumn('text'), false)
	assert.equal(numericColumn('interval'), false)
	assert.equal(numericColumn('varchar'), false)
})
