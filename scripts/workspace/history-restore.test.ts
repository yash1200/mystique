import { beforeEach, expect, test, vi } from 'vitest'

// B-09: restoring a run from history opens a new tab. It never writes into
// the tab the user is editing.
const opened: { connectionId: string; text: string }[] = []
const drafts: Record<string, string> = { 'conn-1::query::a': 'select * from my_work' }

vi.mock('#features/workspace/session', () => ({
	workspaceSession: {
		activeConnectionId: 'conn-1',
		activeTabId: 'conn-1::query::a',
		sessions: [],
		openNewQueryWithText: (connectionId: string, text: string) => {
			opened.push({ connectionId, text })
		},
		setQueryDraft: (tabId: string, text: string) => {
			drafts[tabId] = text
		},
	},
}))
vi.mock('../../src/lib/features/workspace/query/history/history-api', () => ({}))

const { historyStore, selectHistory } = await import(
	'../../src/lib/features/workspace/query/history/history.svelte.ts'
)

beforeEach(() => {
	opened.length = 0
	historyStore.patch('conn-1', {
		entries: [{ id: 'h1', sql: 'select 1', connectionId: 'conn-1' }] as never,
	})
})

test('history restore opens a new tab and leaves the current editor alone', () => {
	selectHistory('h1')
	expect(opened).toEqual([{ connectionId: 'conn-1', text: 'select 1' }])
	expect(drafts['conn-1::query::a']).toBe('select * from my_work')
})
