import assert from 'node:assert/strict'
import { test } from 'vitest'

import { mongodb } from '../../src/lib/engines/mongodb/index.ts'
import { mysql } from '../../src/lib/engines/mysql/index.ts'
import { postgres } from '../../src/lib/engines/postgres/index.ts'
import { sqlite } from '../../src/lib/engines/sqlite/index.ts'
import {
	suggestReadOnly,
	timeoutError,
	timeoutFromText,
	timeoutText,
} from '../../src/lib/features/connections/profile-fields.ts'

test('read-only defaults to off in every engine form (the metadata DB agrees since 009)', () => {
	for (const adapter of [postgres, mysql, sqlite, mongodb]) {
		assert.equal(adapter.connection.emptyDraft().readOnly, false, adapter.id)
	}
})

test('every engine draft sets the timeout field, so the form never binds undefined', () => {
	for (const adapter of [postgres, mysql, sqlite, mongodb]) {
		assert.equal(adapter.connection.emptyDraft().queryTimeout, '', adapter.id)
	}
})

test('production suggests read-only and leaves the choice alone', () => {
	assert.equal(suggestReadOnly({ environment: 'production', readOnly: false }), true)
	assert.equal(suggestReadOnly({ environment: 'production', readOnly: true }), false)
	assert.equal(suggestReadOnly({ environment: 'staging', readOnly: false }), false)
	assert.equal(suggestReadOnly({ environment: 'local', readOnly: false }), false)
})

test('B-08 the profile timeout round-trips; blank follows the app setting', () => {
	assert.equal(timeoutText(null), '')
	assert.equal(timeoutText(undefined), '')
	assert.equal(timeoutFromText(timeoutText(null)), null)
	assert.equal(timeoutText(0), '0')
	assert.equal(timeoutFromText('0'), 0, '0 (no timeout) must not become blank')
	assert.equal(timeoutFromText(timeoutText(900)), 900)
})

test('B-08 the form refuses a timeout that is not whole seconds', () => {
	assert.equal(timeoutError('30'), null)
	assert.equal(timeoutError(''), null)
	assert.equal(timeoutError(undefined), null)
	assert.match(timeoutError('2.5') ?? '', /whole seconds/)
	assert.match(timeoutError('-1') ?? '', /whole seconds/)
})
