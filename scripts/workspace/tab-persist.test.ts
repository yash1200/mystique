import assert from 'node:assert/strict'
import { test } from 'vitest'

import { createPersistScheduler } from '../../src/lib/features/workspace/session/persist-schedule'
import { tabDraftUpsert } from '../../src/lib/features/workspace/session/tab-persist'
import type { WorkspaceTab } from '../../src/lib/features/workspace/session/types'

function tab(id: string, title: string): WorkspaceTab {
	return {
		id,
		connectionId: 'conn',
		kind: 'query',
		title,
		savedQueryId: null,
		dirty: true,
	}
}

test('a keystroke payload contains only that tab body', () => {
	const tabs = [tab('a', 'One'), tab('b', 'Two')]
	const payload = tabDraftUpsert(tabs, 'a', 'select :id', 'a')
	assert.ok(payload)
	assert.equal(payload.id, 'a')
	assert.equal(payload.body, 'select :id')
	assert.equal(payload.isActive, true)
	const encoded = JSON.stringify(payload)
	assert.equal(encoded.includes('other-tab-body'), false)
	assert.equal(encoded.includes('select :id'), true)

	const other = tabDraftUpsert(
		[tab('a', 'One'), { ...tab('b', 'Two') }],
		'b',
		'other-tab-body',
		'a',
	)
	assert.equal(JSON.stringify(payload).includes('other-tab-body'), false)
	assert.equal(other?.body, 'other-tab-body')
	assert.equal(other?.isActive, false)
})

test('a draft timer does not wait on the other tab', async () => {
	const calls: string[] = []
	const drafts = createPersistScheduler((key) => {
		calls.push(key)
	})
	drafts.schedule('tab-a', 20)
	drafts.schedule('tab-b', 20)
	drafts.cancelWhere((key) => key === 'tab-b')
	await new Promise((resolve) => setTimeout(resolve, 40))
	assert.deepEqual(calls, ['tab-a'])
})
