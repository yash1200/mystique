import assert from 'node:assert/strict'
import { test } from 'vitest'

import type { ConnectionProfile } from '../../src/lib/core/contracts/index.ts'
import {
	connectionFacts,
	connectionTarget,
} from '../../src/lib/features/connections/facts.ts'
import {
	groupConnections,
	isListSort,
	isTreatQuery,
	matchesQuery,
} from '../../src/lib/features/connections/list.ts'

function profile(over: Partial<ConnectionProfile> & { id: string }): ConnectionProfile {
	return {
		name: over.id,
		engine: 'postgres',
		colorId: 'green',
		environment: 'local',
		readOnly: false,
		favorite: false,
		passwordSet: false,
		savePassword: true,
		createdAt: '2026-01-01T00:00:00Z',
		lastConnectedAt: null,
		pluginConfig: {},
		...over,
	} as ConnectionProfile
}

const rows = [
	profile({ id: 'b', name: 'beta', environment: 'production' }),
	profile({ id: 'a', name: 'alpha', environment: 'production', favorite: true }),
	profile({ id: 'l', name: 'local-lite', engine: 'sqlite', pluginConfig: { path: '/tmp/app.db' } }),
	profile({ id: 's', name: 'Staging Mongo', engine: 'mongodb', environment: 'staging' }),
	profile({ id: 'c', name: 'charlie', environment: 'production', lastConnectedAt: '2026-11-01T00:00:00Z' }),
]

test('groups Production → Staging → Local and drops empty groups', () => {
	const groups = groupConnections(rows, '', 'name')
	assert.deepEqual(
		groups.map((g) => [g.env, g.items.map((c) => c.id)]),
		[
			['production', ['a', 'b', 'c']],
			['staging', ['s']],
			['local', ['l']],
		],
	)
	assert.deepEqual(
		groupConnections(rows, 'mongo', 'name').map((g) => g.env),
		['staging'],
	)
})

test('favourites lead their group whatever the order', () => {
	const recent = groupConnections(rows, '', 'recent')[0]!
	assert.deepEqual(
		recent.items.map((c) => c.id),
		['a', 'c', 'b'],
		'favourite first, then most recently used, then name',
	)
})

test('the filter matches name, engine, host, database and path, ignoring case', () => {
	const pg = profile({ id: 'h', pluginConfig: { host: 'Prod-Cluster.example.com', database: 'orders' } })
	assert.equal(matchesQuery(pg, 'prod-cluster'), true)
	assert.equal(matchesQuery(pg, 'ORDERS'), true)
	assert.equal(matchesQuery(pg, 'postgres'), true)
	assert.equal(matchesQuery(pg, '  '), true, 'blank matches everything')
	assert.equal(matchesQuery(rows[2]!, 'app.db'), true)
	assert.equal(matchesQuery(pg, 'mysql'), false)
})

test('environment is no longer an order (the list is grouped by it)', () => {
	assert.equal(isListSort('name'), true)
	assert.equal(isListSort('environment'), false)
	assert.equal(isListSort('nope'), false)
})

test('treat words get a treat', () => {
	assert.equal(isTreatQuery(' Woof '), true)
	assert.equal(isTreatQuery('good boy'), true)
	assert.equal(isTreatQuery('woofer'), false)
})

test('the target is host:port, the file, or the URI with its password hidden', () => {
	assert.equal(
		connectionTarget(profile({ id: 'h', pluginConfig: { host: 'db.internal', port: 5432 } })),
		'db.internal:5432',
	)
	assert.equal(connectionTarget(rows[2]!), '/tmp/app.db')
	const mongo = profile({
		id: 'm',
		engine: 'mongodb',
		pluginConfig: { uri: 'mongodb://app:hunter2@cluster0.example.net/shop' },
	})
	assert.equal(connectionTarget(mongo).includes('hunter2'), false)
	assert.equal(connectionTarget(mongo).includes('cluster0.example.net'), true)
})

test('home facts leave the target out and flag unencrypted production', () => {
	const prod = profile({
		id: 'p',
		environment: 'production',
		passwordSet: true,
		pluginConfig: { host: 'db', port: 5432, database: 'qna', username: 'postgres' },
	})
	const facts = connectionFacts(prod, 'full')
	assert.deepEqual(
		facts.map((f) => f.label),
		['Database', 'User', 'Password', 'Port', 'SSL', 'Environment', 'Access', 'Last connected'],
	)
	assert.equal(facts.find((f) => f.label === 'Password')?.value, '••••••••')
	assert.equal(facts.find((f) => f.label === 'SSL')?.warn, 'Not encrypted')
	assert.equal(
		connectionFacts({ ...prod, environment: 'local' }, 'full').find((f) => f.label === 'SSL')?.warn,
		undefined,
	)
	assert.equal(
		connectionFacts({ ...prod, passwordSet: false, savePassword: false }, 'full').find(
			(f) => f.label === 'Password',
		)?.value,
		'Asked on connect',
	)
})
