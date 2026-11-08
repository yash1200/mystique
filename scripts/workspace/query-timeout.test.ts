import assert from 'node:assert/strict'
import { test } from 'vitest'

import {
	parseTimeoutSecs,
	QUERY_TIMEOUT_DEFAULT_SECS,
	QUERY_TIMEOUT_MAX_SECS,
	resolveQueryTimeoutMs,
} from '../../src/lib/app/query-timeout.ts'

test('B-08 the profile timeout wins, then the app setting, then 120 s', () => {
	assert.equal(resolveQueryTimeoutMs(30, 600), 30_000)
	assert.equal(resolveQueryTimeoutMs(null, 600), 600_000)
	assert.equal(resolveQueryTimeoutMs(undefined, undefined), 120_000)
	assert.equal(QUERY_TIMEOUT_DEFAULT_SECS, 120)
})

test('B-08 0 means no timeout and is not skipped as unset', () => {
	assert.equal(resolveQueryTimeoutMs(0, 600), 0)
	assert.equal(resolveQueryTimeoutMs(null, 0), 0)
	// A profile limit still applies when the app setting is "none".
	assert.equal(resolveQueryTimeoutMs(5, 0), 5_000)
})

test('B-08 only whole seconds in range parse', () => {
	assert.equal(parseTimeoutSecs('45'), 45)
	assert.equal(parseTimeoutSecs(' 0 '), 0)
	assert.equal(parseTimeoutSecs(''), null)
	assert.equal(parseTimeoutSecs('1.5'), null)
	assert.equal(parseTimeoutSecs('-1'), null)
	assert.equal(parseTimeoutSecs(String(QUERY_TIMEOUT_MAX_SECS + 1)), null)
	// A bad stored value falls through to the next source.
	assert.equal(resolveQueryTimeoutMs(Number.NaN, 60), 60_000)
})
