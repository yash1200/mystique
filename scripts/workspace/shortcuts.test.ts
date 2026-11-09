import { afterEach, describe, expect, it, vi } from 'vitest'
import {
	closeTopDialog,
	openDialogCount,
	pushDialog,
} from '../../src/lib/core/ui/dialog-stack'
import {
	CLOSED_TAB_LIMIT,
	closedTabEntry,
	forgetConnection,
	rememberClosed,
	reopenTab,
	takeClosed,
} from '../../src/lib/features/workspace/session/closed-tabs'
import type { WorkspaceTab } from '../../src/lib/features/workspace/session/types'
import { keymapHarness, type Harness } from './keymap-harness'

const cmd = { metaKey: true }

const harnesses: Harness[] = []
afterEach(() => {
	for (const h of harnesses.splice(0)) h.dispose()
})

function harness(opts: Parameters<typeof keymapHarness>[0]): Harness {
	const h = keymapHarness(opts)
	harnesses.push(h)
	return h
}

describe('workspace tab chords', () => {
	it('leaves tab chords to the native menu', () => {
		const h = harness({ mac: true })
		for (const code of ['KeyT', 'KeyS', 'KeyW', 'KeyO']) h.press({ ...cmd, code })
		h.press({ ...cmd, code: 'KeyT', shiftKey: true })
		expect(h.ran).toEqual([])
		expect(h.runMenu('workspace_close_tab')).toEqual(['workspace.close-tab'])
		expect(h.runMenu('workspace_reopen_tab')).toEqual(['workspace.reopen-tab'])
		expect(h.runMenu('workspace_save_query')).toEqual(['workspace.save'])
	})
})

describe('app chords', () => {
	it('keeps the reset confirm on keydown', () => {
		const h = harness({ mac: true })
		expect(h.press({ ...cmd, shiftKey: true, code: 'KeyR', key: 'R' })).toEqual(['app.reset'])
		expect(h.press({ ctrlKey: true, shiftKey: true, code: 'KeyR' })).toEqual([])
	})

	it('leaves settings and zoom to the menu', () => {
		const only = ['app.settings', 'app.zoom-in', 'app.zoom-reset', 'app.reset']
		const h = harness({ mac: true, only })
		expect(h.press({ ...cmd, code: 'Comma', key: ',' })).toEqual([])
		expect(h.press({ ...cmd, code: 'Equal', key: '=' })).toEqual([])
		expect(h.press({ ...cmd, code: 'KeyR', key: 'r' })).toEqual([])
		expect(h.runMenu('app_settings')).toEqual(['app.settings'])
		expect(h.runMenu('view_zoom_in')).toEqual(['app.zoom-in'])
	})

	it('lets an open diagram take the zoom chords from the app', () => {
		const h = harness({
			mac: true,
			only: ['app.zoom-in', 'er.zoom-in'],
		})
		expect(h.press({ ...cmd, code: 'Equal', key: '=' })).toEqual(['er.zoom-in'])
	})
})

describe('dialog stack', () => {
	const cleanup: Array<() => void> = []
	afterEach(() => {
		for (const done of cleanup.splice(0)) done()
	})

	it('closes the newest dialog only', () => {
		const first = vi.fn()
		const second = vi.fn()
		cleanup.push(pushDialog({ close: first, dismissible: () => true }))
		cleanup.push(pushDialog({ close: second, dismissible: () => true }))
		expect(closeTopDialog()).toBe(true)
		expect(second).toHaveBeenCalledTimes(1)
		expect(first).not.toHaveBeenCalled()
	})

	it('reports a busy dialog as open without closing it', () => {
		const close = vi.fn()
		cleanup.push(pushDialog({ close, dismissible: () => false }))
		expect(closeTopDialog()).toBe(true)
		expect(close).not.toHaveBeenCalled()
	})

	it('lets the tab close when no dialog is open', () => {
		const remove = pushDialog({ close: () => {}, dismissible: () => true })
		remove()
		expect(openDialogCount()).toBe(0)
		expect(closeTopDialog()).toBe(false)
	})
})

describe('reopen closed tab', () => {
	const query: WorkspaceTab = {
		id: 'c1::query::a',
		connectionId: 'c1',
		kind: 'query',
		title: 'Orders',
		savedQueryId: 's1',
		dirty: true,
	}
	const table: WorkspaceTab = {
		id: 'c1::table::db.public.users',
		connectionId: 'c1',
		kind: 'table',
		tableId: 'db.public.users',
		title: 'users',
		dirty: true,
		preview: { filters: [], codeFilter: 'id > 3', sort: null, offset: 100 },
	}

	it('keeps the editor text and the saved body it differs from', () => {
		const entry = closedTabEntry(query, 1, { [query.id]: 'select 2' }, { [query.id]: 'select 1' })
		const open: WorkspaceTab[] = [table]
		const back = reopenTab(open, entry)
		expect(back.tab).toMatchObject({
			id: query.id,
			kind: 'query',
			savedQueryId: 's1',
			title: 'Orders',
			dirty: true,
		})
		expect(entry.text).toBe('select 2')
		expect(entry.baseline).toBe('select 1')
		expect(back.openTabs.map((tab) => tab.id)).toEqual([table.id, query.id])
	})

	it('reopens a table on the same target without its filters or edits', () => {
		const entry = closedTabEntry(table, 0, {}, {})
		const back = reopenTab([query], entry)
		expect(back.tab.tableId).toBe('db.public.users')
		expect(back.tab.preview).toBeUndefined()
		expect(back.tab.dirty).toBe(false)
		expect(back.openTabs[0]?.id).toBe(table.id)
	})

	it('takes the newest tab whose connection is live and that is not open', () => {
		const a = closedTabEntry(query, 0, {}, {})
		const b = closedTabEntry({ ...table, connectionId: 'gone' }, 0, {}, {})
		const c = closedTabEntry(table, 0, {}, {})
		const stack = [a, b, c]
		const taken = takeClosed(
			stack,
			(id) => id === 'c1',
			(id) => id === table.id,
		)
		expect(taken.entry?.tab.id).toBe(query.id)
		expect(taken.rest).toEqual([])
		expect(takeClosed([], () => true, () => false).entry).toBeNull()
	})

	it('keeps one entry per tab, caps the list, and forgets a disconnected connection', () => {
		let stack = rememberClosed([], closedTabEntry(query, 0, {}, {}))
		stack = rememberClosed(stack, closedTabEntry(query, 2, {}, {}))
		expect(stack).toHaveLength(1)
		expect(stack[0]?.index).toBe(2)
		for (let i = 0; i < CLOSED_TAB_LIMIT + 5; i++) {
			stack = rememberClosed(stack, closedTabEntry({ ...table, id: `t${i}` }, 0, {}, {}))
		}
		expect(stack).toHaveLength(CLOSED_TAB_LIMIT)
		expect(stack.at(-1)?.tab.id).toBe(`t${CLOSED_TAB_LIMIT + 4}`)
		expect(forgetConnection(stack, 'c1')).toEqual([])
	})
})
