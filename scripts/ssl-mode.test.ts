import assert from 'node:assert/strict'
import { test } from 'node:test'

import { sslModeFromConfig, sslModeRejection } from '../src/lib/engines/ssl-mode.ts'

test('ssl true with disable is stored as require', () => {
	assert.equal(sslModeFromConfig(true, 'disable'), 'require')
	assert.equal(sslModeFromConfig(true, ''), 'require')
	assert.equal(sslModeFromConfig(false, 'disable'), 'disable')
	assert.equal(sslModeFromConfig(true, 'verify-full'), 'verify-full')
})

test('mysql and mongo reject prefer, mongo rejects verify-ca', () => {
	assert.match(sslModeRejection('mysql', 'prefer') ?? '', /MySQL cannot try TLS/)
	assert.match(
		sslModeRejection('documentdb', 'verify-ca') ?? '',
		/DocumentDB verify-ca/,
	)
	assert.equal(sslModeRejection('postgres', 'prefer'), null)
	assert.equal(sslModeRejection('postgres', 'verify-ca'), null)
})
