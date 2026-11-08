import assert from 'node:assert/strict'
import { test } from 'vitest'

import { resolveImportGate } from '../../src/lib/features/workspace/grid/import-gate.ts'
import {
	closeStep,
	hasPendingLoss,
	pendingLoss,
	saveThenClose,
	tabTxnView,
} from '../../src/lib/features/workspace/shell/close-guard.ts'
import {
	registerTabSaver,
	saveTableTab,
	TAB_NOT_OPEN,
} from '../../src/lib/features/workspace/shell/tab-save.ts'

test('R-23 Save in the close prompt closes only after the grid save committed', async () => {
	let closed = 0
	const failed = await saveThenClose(
		async () => ({ ok: false, error: 'duplicate key' }),
		() => closed++,
	)
	assert.deepEqual(failed, { ok: false, error: 'duplicate key' })
	assert.equal(closed, 0)

	const cancelled = await saveThenClose(async () => ({ ok: false, error: null }), () => closed++)
	assert.equal(cancelled.ok, false)
	assert.equal(closed, 0)

	const thrown = await saveThenClose(async () => {
		throw new Error('connection lost')
	}, () => closed++)
	assert.deepEqual(thrown, { ok: false, error: 'connection lost' })
	assert.equal(closed, 0)

	await saveThenClose(async () => ({ ok: true }), () => closed++)
	assert.equal(closed, 1)
})

test('R-23 the close prompt runs the registered grid save for that tab', async () => {
	let calls = 0
	const unregister = registerTabSaver('t1', async () => {
		calls++
		return { ok: true }
	})
	assert.deepEqual(await saveTableTab('t1'), { ok: true })
	assert.equal(calls, 1)
	unregister()
	assert.deepEqual(await saveTableTab('t1', 10), { ok: false, error: TAB_NOT_OPEN })
})

test('R-23 a save requested before the grid mounts waits for it', async () => {
	const pending = saveTableTab('t2', 1_000)
	const unregister = registerTabSaver('t2', async () => ({ ok: true }))
	assert.deepEqual(await pending, { ok: true })
	unregister()
})

test('R-25 closing a tab with an open transaction asks before anything else', () => {
	const open = { pinned: true, inTransaction: true }
	assert.equal(closeStep({ kind: 'query', dirty: true }, open), 'transaction')
	assert.equal(closeStep({ kind: 'query', dirty: false }, open), 'transaction')
	assert.equal(
		closeStep({ kind: 'query', dirty: true }, { pinned: true, inTransaction: false }),
		'unsaved',
	)
	assert.equal(closeStep({ kind: 'query', dirty: false }, undefined), 'close')
	assert.equal(closeStep({ kind: 'table', dirty: true }, undefined), 'unsaved')
})

test('R-24 reset asks only when edits or transactions would be lost, and lists them', () => {
	const tabs = [
		{ id: 'q1', kind: 'query' as const, title: 'Query 1', dirty: false },
		{ id: 'q2', kind: 'query' as const, title: 'Query 2', dirty: false },
		{ id: 't1', kind: 'table' as const, title: 'users', dirty: false },
	]
	const clean = pendingLoss(tabs, { q1: { pinned: true, inTransaction: false } })
	assert.equal(hasPendingLoss(clean), false)

	const loss = pendingLoss(
		tabs.map((tab) => (tab.id === 't1' ? { ...tab, dirty: true } : tab)),
		{ q2: { pinned: true, inTransaction: true } },
	)
	assert.equal(hasPendingLoss(loss), true)
	assert.deepEqual(loss, { dirty: ['users'], transactions: ['Query 2'] })
})

test('R-26 import with pending edits: save first, discard, or cancel', async () => {
	const log: string[] = []
	const deps = (ok: boolean) => ({
		save: async () => {
			log.push('save')
			return ok ? { ok: true as const } : { ok: false as const, error: 'save failed' }
		},
		discard: () => {
			log.push('discard')
		},
	})
	assert.deepEqual(await resolveImportGate('cancel', deps(true)), { proceed: false, error: null })
	assert.deepEqual(await resolveImportGate('save', deps(false)), {
		proceed: false,
		error: 'save failed',
	})
	assert.deepEqual(await resolveImportGate('save', deps(true)), { proceed: true, error: null })
	assert.deepEqual(await resolveImportGate('discard', deps(true)), { proceed: true, error: null })
	assert.deepEqual(log, ['save', 'save', 'discard'])
})

test('R-27 a tab switch reads that tab’s transaction state with no async gap', () => {
	const txns = {
		a: { pinned: false, inTransaction: false },
		b: { pinned: true, inTransaction: true },
	}
	assert.deepEqual(tabTxnView(txns, 'b'), { manualCommit: true, transactionOpen: true })
	assert.deepEqual(tabTxnView(txns, 'a'), { manualCommit: false, transactionOpen: false })
	assert.deepEqual(tabTxnView(txns, 'new'), { manualCommit: false, transactionOpen: false })
})
