import assert from 'node:assert/strict'
import { test } from 'vitest'

import { chatAfterConnectionChange } from '../../src/lib/features/workspace/agent/connection-chat.ts'
import { recordCatalogFailure } from '../../src/lib/features/workspace/catalog/failure.ts'
import { identityCell } from '../../src/lib/features/workspace/grid/identity-cell.ts'
import {
	adoptServerRows,
	bufferIsDirty,
	clearEditBuffers,
	confirmServerRefresh,
	getEditBuffer,
	rememberEditBuffer,
	reopenTableAction,
	savePayload,
	shouldRestoreEditBuffer,
} from '../../src/lib/features/workspace/shell/edit-buffer.ts'
import {
	createPersistScheduler,
	tabsForConnection,
} from '../../src/lib/features/workspace/shell/persist-schedule.ts'

test('FE-01 remount keeps the edit and the pre-edit baseline', () => {
	clearEditBuffers()
	adoptServerRows('tab', [{ id: 1, name: 'a' }], 2)
	const saved = getEditBuffer('tab')
	assert.ok(saved)
	rememberEditBuffer('tab', {
		baseline: saved.baseline,
		rows: [{ id: 1, name: 'b' }],
		dirtyKeys: ['0::name'],
		newRowIndices: [],
		deletedIndices: [],
		page: saved.page,
	})
	const again = getEditBuffer('tab')
	assert.ok(again)
	assert.equal(again.baseline[0]?.name, 'a')
	assert.equal(again.rows[0]?.name, 'b')
	assert.equal(bufferIsDirty(again), true)
	const payload = savePayload(again)
	assert.equal(payload.dirty, true)
	assert.equal(payload.baseline[0]?.name, 'a')
	assert.equal(payload.rows[0]?.name, 'b')
	assert.equal(again.page, 2)
})

test('FE-02 unconfirmed refresh keeps edits; confirmed refresh keeps page', () => {
	clearEditBuffers()
	adoptServerRows('tab', [{ id: 1, name: 'a' }], 2)
	const saved = getEditBuffer('tab')
	assert.ok(saved)
	rememberEditBuffer('tab', {
		baseline: saved.baseline,
		rows: [{ id: 1, name: 'b' }],
		dirtyKeys: ['0::name'],
		newRowIndices: [],
		deletedIndices: [],
		page: 2,
	})
	const untouched = getEditBuffer('tab')
	assert.equal(untouched?.rows[0]?.name, 'b')
	assert.equal(bufferIsDirty(untouched!), true)

	const refreshed = confirmServerRefresh('tab', [{ id: 1, name: 'server' }])
	assert.equal(refreshed.page, 2)
	assert.equal(refreshed.rows[0]?.name, 'server')
	assert.equal(bufferIsDirty(refreshed), false)

	rememberEditBuffer('tab', {
		baseline: refreshed.baseline,
		rows: [{ id: 1, name: 'edited' }],
		dirtyKeys: ['0::name'],
		newRowIndices: [],
		deletedIndices: [],
		page: 2,
	})
	const jumped = confirmServerRefresh('tab', [{ id: 1, name: 'server' }], true)
	assert.equal(jumped.page, 0)
})

test('a clean empty buffer does not hide a loaded page', () => {
	clearEditBuffers()
	adoptServerRows('tab', [], 0)
	const leaf = getEditBuffer('tab')
	assert.equal(shouldRestoreEditBuffer(leaf, 0), true)
	assert.equal(shouldRestoreEditBuffer(leaf, 4), false)
	assert.equal(shouldRestoreEditBuffer(undefined, 4), false)

	rememberEditBuffer('tab', {
		baseline: [{ id: 1 }],
		rows: [{ id: 1, name: 'kept' }],
		dirtyKeys: [],
		newRowIndices: [],
		deletedIndices: [],
		page: 0,
	})
	assert.equal(shouldRestoreEditBuffer(getEditBuffer('tab'), 4), true)

	rememberEditBuffer('tab', {
		baseline: [{ id: 1 }],
		rows: [],
		dirtyKeys: [],
		newRowIndices: [],
		deletedIndices: [0],
		page: 0,
	})
	assert.equal(shouldRestoreEditBuffer(getEditBuffer('tab'), 4), true)
})

test('FE-03 a dirty tab is focused and a clean tab can reload', () => {
	clearEditBuffers()
	adoptServerRows('tab', [{ id: 1, name: 'a' }])
	assert.equal(reopenTableAction('tab'), 'load')
	const saved = getEditBuffer('tab')
	assert.ok(saved)
	rememberEditBuffer('tab', {
		baseline: saved.baseline,
		rows: [{ id: 1, name: 'b' }],
		dirtyKeys: ['0::name'],
		newRowIndices: [],
		deletedIndices: [],
		page: 0,
	})
	assert.equal(reopenTableAction('tab'), 'focus')
	assert.equal(getEditBuffer('tab')?.rows[0]?.name, 'b')
})

test('FE-07 disconnect cancels the pending persist timer', async () => {
	const saves: number[] = []
	let tabs = [{ id: 't', connectionId: 'c' }]
	const scheduler = createPersistScheduler(() => {
		saves.push(tabsForConnection(tabs, 'c').length)
	})
	scheduler.schedule('c', 30)
	assert.equal(scheduler.pending('c'), true)
	const snapshot = tabsForConnection(tabs, 'c')
	assert.equal(scheduler.cancel('c'), true)
	tabs = []
	await new Promise((resolve) => setTimeout(resolve, 50))
	assert.deepEqual(saves, [])
	assert.equal(snapshot.length, 1)
	assert.equal(tabsForConnection(tabs, 'c').length, 0)
})

test('FE-13 text that looks like JSON stays the original value', () => {
	assert.equal(identityCell('"x"'), '"x"')
	assert.equal(identityCell('{ "a": 1 }'), '{ "a": 1 }')
	assert.equal(identityCell({ $oid: 'abc' }), 'abc')
	assert.equal(identityCell({ $uuid: 'u' }), 'u')
	assert.equal(identityCell('plain'), 'plain')
})

test('FE-04 switching connections drops the previous chat id', () => {
	const next = chatAfterConnectionChange(
		{
			connectionId: 'a',
			chatId: 'chat-a',
			chats: [{ id: 'chat-a' }],
			messages: [{ id: 'm' }],
		},
		'b',
	)
	assert.equal(next.connectionId, 'b')
	assert.equal(next.chatId, null)
	assert.deepEqual(next.chats, [])
	assert.deepEqual(next.messages, [])
	const same = chatAfterConnectionChange(next, 'b')
	assert.equal(same, next)
})

test('FE-05 a catalog error is recorded once', () => {
	assert.equal(recordCatalogFailure('c', 'no listDatabases'), true)
	assert.equal(recordCatalogFailure('c', 'no listDatabases'), false)
	assert.equal(recordCatalogFailure('c', 'other'), true)
})
