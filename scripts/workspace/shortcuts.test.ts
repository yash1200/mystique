import { afterEach, describe, expect, it, vi } from 'vitest'
import {
	closeTopDialog,
	openDialogCount,
	pushDialog,
} from '../../src/lib/core/ui/dialog-stack'
import { isModChord } from '../../src/lib/core/util/platform'
import { appKeyCommand } from '../../src/lib/features/shortcuts/app-keys'
import {
	CLOSED_TAB_LIMIT,
	closedTabEntry,
	forgetConnection,
	rememberClosed,
	reopenTab,
	takeClosed,
} from '../../src/lib/features/workspace/session/closed-tabs'
import {
	mountWorkspaceShortcuts,
	workspaceChord,
} from '../../src/lib/features/workspace/shell/shortcuts'
import type { WorkspaceTab } from '../../src/lib/features/workspace/session/types'

const cmd = {
	metaKey: true,
	ctrlKey: false,
	altKey: false,
	shiftKey: false,
	code: 'KeyW',
	key: 'w',
}

describe('command modifier', () => {
	it('is cmd alone on the Mac and ctrl alone elsewhere', () => {
		expect(isModChord({ metaKey: true, ctrlKey: false }, true)).toBe(true)
		expect(isModChord({ metaKey: false, ctrlKey: true }, true)).toBe(false)
		expect(isModChord({ metaKey: true, ctrlKey: true }, true)).toBe(false)
		expect(isModChord({ metaKey: false, ctrlKey: true }, false)).toBe(true)
		expect(isModChord({ metaKey: true, ctrlKey: false }, false)).toBe(false)
	})
})

describe('workspace tab chords', () => {
	it('maps each chord to one command', () => {
		expect(workspaceChord({ ...cmd, code: 'KeyT' }, true)).toBe('new-query')
		expect(workspaceChord({ ...cmd, code: 'KeyS' }, true)).toBe('save')
		expect(workspaceChord({ ...cmd, code: 'KeyW' }, true)).toBe('close-tab')
		expect(workspaceChord({ ...cmd, code: 'KeyO' }, true)).toBe('open-connection')
		expect(workspaceChord({ ...cmd, shiftKey: true, code: 'KeyT' }, true)).toBe(
			'reopen-tab',
		)
		expect(workspaceChord({ ...cmd, shiftKey: true, code: 'KeyW' }, true)).toBeNull()
		expect(
			workspaceChord({ ...cmd, metaKey: false, ctrlKey: true, code: 'KeyT' }, true),
		).toBeNull()
	})

	function press(code: string, shiftKey = false) {
		document.body.dispatchEvent(
			new KeyboardEvent('keydown', {
				code,
				metaKey: true,
				shiftKey,
				bubbles: true,
				cancelable: true,
			}),
		)
	}

	it('runs a keydown command once in the browser preview', () => {
		const close = vi.fn()
		const reopen = vi.fn()
		const stop = mountWorkspaceShortcuts({
			commands: { 'close-tab': close, 'reopen-tab': reopen },
			isTauri: false,
			mac: true,
		})
		press('KeyW')
		press('KeyT', true)
		stop()
		press('KeyW')
		expect(close).toHaveBeenCalledTimes(1)
		expect(reopen).toHaveBeenCalledTimes(1)
	})

	it('leaves tab chords to the native menu in the desktop app', () => {
		const close = vi.fn()
		const stop = mountWorkspaceShortcuts({
			commands: { 'close-tab': close },
			isTauri: true,
			mac: true,
		})
		press('KeyW')
		stop()
		expect(close).not.toHaveBeenCalled()
	})
})

describe('app chords', () => {
	const desktop = { mac: true, nativeMenu: true }
	const browser = { mac: true, nativeMenu: false }

	it('keeps the reset confirm on keydown everywhere', () => {
		const reset = { ...cmd, shiftKey: true, code: 'KeyR', key: 'R' }
		expect(appKeyCommand(reset, desktop)).toBe('reset')
		expect(appKeyCommand(reset, browser)).toBe('reset')
		expect(appKeyCommand({ ...reset, metaKey: false, ctrlKey: true }, desktop)).toBeNull()
	})

	it('leaves settings and zoom to the menu in the desktop app', () => {
		const comma = { ...cmd, code: 'Comma', key: ',' }
		const plus = { ...cmd, code: 'Equal', key: '=' }
		expect(appKeyCommand(comma, desktop)).toBeNull()
		expect(appKeyCommand(plus, desktop)).toBeNull()
		expect(appKeyCommand(comma, browser)).toBe('settings')
		expect(appKeyCommand(plus, browser)).toBe('zoom-in')
		expect(appKeyCommand({ ...cmd, code: 'Digit0', key: '0' }, browser)).toBe('zoom-reset')
		expect(appKeyCommand({ ...cmd, code: 'KeyR', key: 'r' }, browser)).toBeNull()
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
