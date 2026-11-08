import { readFileSync } from 'node:fs'
import path from 'node:path'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cmd, KEYMAP } from '../../src/lib/app/commands'
import { bindingsFor, keyLabel, matchesBinding } from '../../src/lib/core/commands/keys'
import {
	createCommandRegistry,
	defineCommand,
	type CommandRegistry,
} from '../../src/lib/core/commands/registry'
import { pushDialog } from '../../src/lib/core/ui/dialog-stack'

function key(init: Partial<KeyboardEventInit> & { code?: string; key?: string }) {
	return {
		key: init.key ?? '',
		code: init.code ?? '',
		metaKey: init.metaKey ?? false,
		ctrlKey: init.ctrlKey ?? false,
		altKey: init.altKey ?? false,
		shiftKey: init.shiftKey ?? false,
	}
}

function registry(opts: { mac?: boolean; nativeMenu?: boolean; dialog?: () => boolean } = {}) {
	return createCommandRegistry({
		mac: () => opts.mac ?? true,
		nativeMenu: () => opts.nativeMenu ?? false,
		dialogOpen: opts.dialog ?? (() => false),
	})
}

const cleanup: Array<() => void> = []
afterEach(() => {
	for (const done of cleanup.splice(0)) done()
	document.body.innerHTML = ''
})

/** Press a chord on `target` and let the registry's window listener see it. */
function press(
	target: EventTarget,
	init: KeyboardEventInit & { code?: string; key?: string },
): KeyboardEvent {
	const event = new KeyboardEvent('keydown', {
		bubbles: true,
		cancelable: true,
		...init,
	})
	target.dispatchEvent(event)
	return event
}

function mounted(reg: CommandRegistry): CommandRegistry {
	cleanup.push(reg.mount(window))
	return reg
}

describe('key matching', () => {
	it('uses cmd on the Mac and ctrl elsewhere; ctrl is not cmd on the Mac', () => {
		const cmdK = key({ code: 'KeyK', key: 'k', metaKey: true })
		const ctrlK = key({ code: 'KeyK', key: 'k', ctrlKey: true })
		expect(matchesBinding(cmdK, 'Mod+K', true)).toBe(true)
		expect(matchesBinding(ctrlK, 'Mod+K', true)).toBe(false)
		expect(matchesBinding(ctrlK, 'Mod+K', false)).toBe(true)
		expect(matchesBinding(cmdK, 'Mod+K', false)).toBe(false)
		expect(matchesBinding({ ...cmdK, ctrlKey: true }, 'Mod+K', true)).toBe(false)
	})

	it('needs the exact modifiers', () => {
		const t = key({ code: 'KeyT', key: 't', metaKey: true })
		expect(matchesBinding(t, 'Mod+T', true)).toBe(true)
		expect(matchesBinding({ ...t, shiftKey: true }, 'Mod+T', true)).toBe(false)
		expect(matchesBinding({ ...t, shiftKey: true }, 'Mod+Shift+T', true)).toBe(true)
		expect(matchesBinding({ ...t, altKey: true }, 'Mod+T', true)).toBe(false)
	})

	it('matches letters by code, so the layout does not matter', () => {
		// A Dvorak-style layout: the key labelled "k" sits on KeyV.
		expect(matchesBinding(key({ code: 'KeyV', key: 'k', metaKey: true }), 'Mod+K', true)).toBe(
			false,
		)
		expect(matchesBinding(key({ code: 'KeyK', key: 'т', metaKey: true }), 'Mod+K', true)).toBe(
			true,
		)
	})

	it('matches symbols by key or code', () => {
		const shiftBracket = key({ code: 'BracketLeft', key: '{', metaKey: true, shiftKey: true })
		expect(matchesBinding(shiftBracket, 'Mod+Shift+[', true)).toBe(true)
		expect(matchesBinding(key({ code: 'Equal', key: '=', metaKey: true }), 'Mod+=', true)).toBe(true)
		expect(
			matchesBinding(key({ code: 'Equal', key: '+', metaKey: true, shiftKey: true }), 'Mod+=', true),
		).toBe(false)
		expect(matchesBinding(key({ code: 'Comma', key: ',', metaKey: true }), 'Mod+,', true)).toBe(true)
		// `/` needs Shift on some layouts.
		expect(matchesBinding(key({ code: 'Digit7', key: '/', shiftKey: true }), '/', true)).toBe(true)
		expect(matchesBinding(key({ key: 'Enter', code: 'NumpadEnter', metaKey: true }), 'Mod+Enter', true)).toBe(
			true,
		)
		expect(matchesBinding(key({ key: 'Escape', code: 'Escape' }), 'Escape', true)).toBe(true)
	})

	it('resolves per-platform bindings', () => {
		const keys = [{ mac: 'Meta+Shift+P' }, 'Mod+P']
		expect(bindingsFor(keys, true)).toEqual(['Meta+Shift+P', 'Mod+P'])
		expect(bindingsFor(keys, false)).toEqual(['Mod+P'])
		expect(keyLabel('mod', true)).toBe('Cmd')
		expect(keyLabel('mod', false)).toBe('Ctrl')
	})
})

describe('registry dispatch', () => {
	it('runs the newest handler whose when passes, then the older one', () => {
		const reg = registry()
		const spec = defineCommand({ id: 'x.save', title: 'Save', keys: 'Mod+S' })
		const base = vi.fn()
		const table = vi.fn()
		let tableActive = true
		cleanup.push(reg.register({ ...spec, run: base }))
		cleanup.push(reg.register({ ...spec, when: () => tableActive, run: table }))
		expect(reg.run('x.save')).toBe(true)
		tableActive = false
		expect(reg.run(spec)).toBe(true)
		expect(table).toHaveBeenCalledTimes(1)
		expect(base).toHaveBeenCalledTimes(1)
	})

	it('passes args to a command run from code', () => {
		const reg = registry()
		const open = defineCommand<{ section?: string }>({ id: 'app.settings', title: 'Settings' })
		const show = vi.fn()
		cleanup.push(reg.register({ ...open, run: ({ args }) => show(args?.section) }))
		reg.run(open, { section: 'ai' })
		expect(show).toHaveBeenCalledWith('ai')
	})

	it('unregisters, and reports nothing to run', () => {
		const reg = registry()
		const run = vi.fn()
		const off = reg.register({ id: 'a.b', title: 'A', run })
		off()
		expect(reg.run('a.b')).toBe(false)
		expect(reg.available('a.b')).toBe(false)
		expect(reg.get('a.b')).toBeUndefined()
		expect(run).not.toHaveBeenCalled()
	})

	it('takes keys, menu and scope from the declared keymap', () => {
		const reg = mounted(registry())
		cleanup.push(reg.declare([{ id: 'a.go', title: 'Go', keys: 'Mod+G' }]))
		const run = vi.fn()
		// The handler's own keys are ignored once the id is declared.
		cleanup.push(reg.register({ id: 'a.go', title: 'ignored', keys: 'Mod+J', run }))
		press(document.body, { code: 'KeyJ', metaKey: true })
		press(document.body, { code: 'KeyG', metaKey: true })
		expect(run).toHaveBeenCalledTimes(1)
		expect(reg.list().map((s) => s.title)).toEqual(['Go'])
	})

	it('stops the keydown it handles and runs once per press', () => {
		const reg = mounted(registry())
		const run = vi.fn()
		const later = vi.fn()
		cleanup.push(reg.register({ id: 'a.k', title: 'K', keys: 'Mod+K', run }))
		window.addEventListener('keydown', later, true)
		cleanup.push(() => window.removeEventListener('keydown', later, true))
		const event = press(document.body, { code: 'KeyK', metaKey: true })
		press(document.body, { code: 'KeyK', metaKey: true, repeat: true })
		expect(run).toHaveBeenCalledTimes(1)
		expect(event.defaultPrevented).toBe(true)
		expect(later).not.toHaveBeenCalled()
	})

	it('leaves an unmatched chord alone', () => {
		const reg = mounted(registry())
		cleanup.push(reg.register({ id: 'a.k', title: 'K', keys: 'Mod+K', run: vi.fn() }))
		const event = press(document.body, { code: 'KeyP', metaKey: true })
		expect(event.defaultPrevented).toBe(false)
	})
})

describe('focus scopes', () => {
	function editor(): HTMLElement {
		const root = document.createElement('div')
		root.className = 'cm-editor'
		const content = document.createElement('div')
		content.contentEditable = 'true'
		content.className = 'cm-content'
		root.append(content)
		document.body.append(root)
		return content
	}

	it('leaves cmd L to the editor and runs it elsewhere', () => {
		const reg = mounted(registry())
		cleanup.push(reg.declare([cmd.focusFilter]))
		const focus = vi.fn()
		cleanup.push(reg.register({ ...cmd.focusFilter, run: focus }))

		const inEditor = press(editor(), { code: 'KeyL', metaKey: true })
		expect(focus).not.toHaveBeenCalled()
		expect(inEditor.defaultPrevented).toBe(false)

		const input = document.createElement('input')
		document.body.append(input)
		press(input, { code: 'KeyL', metaKey: true })
		expect(focus).not.toHaveBeenCalled()

		press(document.body, { code: 'KeyL', metaKey: true })
		expect(focus).toHaveBeenCalledTimes(1)
	})

	it('keeps quick open on cmd K inside the editor, and cmd P free', () => {
		const reg = mounted(registry())
		cleanup.push(reg.declare([cmd.quickOpen]))
		const toggle = vi.fn()
		cleanup.push(reg.register({ ...cmd.quickOpen, run: toggle }))
		const target = editor()
		press(target, { code: 'KeyK', metaKey: true })
		const p = press(target, { code: 'KeyP', metaKey: true })
		press(target, { code: 'KeyK', ctrlKey: true })
		expect(toggle).toHaveBeenCalledTimes(1)
		expect(p.defaultPrevented).toBe(false)
	})

	it('blocks global commands while a dialog is open, except the ones that allow it', async () => {
		const { openDialogCount } = await import('../../src/lib/core/ui/dialog-stack')
		const reg = mounted(registry({ dialog: () => openDialogCount() > 0 }))
		cleanup.push(reg.declare([cmd.toggleSidebar, cmd.closeTab, cmd.quickOpen]))
		const toggle = vi.fn()
		const close = vi.fn()
		const quick = vi.fn()
		cleanup.push(
			reg.register([
				{ ...cmd.toggleSidebar, run: toggle },
				{ ...cmd.closeTab, run: close },
				{ ...cmd.quickOpen, run: quick },
			]),
		)
		const done = pushDialog({ close: () => {}, dismissible: () => true })
		cleanup.push(done)
		const b = press(document.body, { code: 'KeyB', metaKey: true })
		press(document.body, { code: 'KeyW', metaKey: true })
		press(document.body, { code: 'KeyK', metaKey: true })
		expect(toggle).not.toHaveBeenCalled()
		expect(b.defaultPrevented).toBe(false)
		expect(close).toHaveBeenCalledTimes(1)
		expect(quick).toHaveBeenCalledTimes(1)
		done()
		press(document.body, { code: 'KeyB', metaKey: true })
		expect(toggle).toHaveBeenCalledTimes(1)
	})

	it('treats focus inside a role=dialog element as a dialog', () => {
		const reg = mounted(registry())
		const run = vi.fn()
		cleanup.push(reg.register({ id: 'a.b', title: 'B', keys: 'Mod+B', run }))
		const dialog = document.createElement('div')
		dialog.setAttribute('role', 'dialog')
		const button = document.createElement('button')
		dialog.append(button)
		document.body.append(dialog)
		press(button, { code: 'KeyB', metaKey: true })
		expect(run).not.toHaveBeenCalled()
	})

	it('gives the when check the focus', () => {
		const reg = mounted(registry())
		const run = vi.fn()
		cleanup.push(
			reg.register({
				id: 'q.run',
				title: 'Run',
				keys: 'Mod+Enter',
				scope: { editor: false },
				when: ({ focus }) => !focus?.target?.closest('textarea'),
				run,
			}),
		)
		const area = document.createElement('textarea')
		document.body.append(area)
		press(area, { key: 'Enter', metaKey: true })
		press(editor(), { key: 'Enter', metaKey: true })
		expect(run).not.toHaveBeenCalled()
		press(document.body, { key: 'Enter', metaKey: true })
		expect(run).toHaveBeenCalledTimes(1)
	})
})

describe('platforms', () => {
	it('runs on ctrl off the Mac and ignores cmd there', () => {
		const reg = mounted(registry({ mac: false }))
		const run = vi.fn()
		cleanup.push(reg.register({ id: 'a.t', title: 'T', keys: 'Mod+Shift+T', run }))
		press(document.body, { code: 'KeyT', metaKey: true, shiftKey: true })
		expect(run).not.toHaveBeenCalled()
		press(document.body, { code: 'KeyT', ctrlKey: true, shiftKey: true })
		expect(run).toHaveBeenCalledTimes(1)
	})

	it('does not treat ctrl as cmd on the Mac', () => {
		const reg = mounted(registry({ mac: true }))
		const run = vi.fn()
		cleanup.push(reg.register({ id: 'a.t', title: 'T', keys: 'Mod+T', run }))
		const event = press(document.body, { code: 'KeyT', ctrlKey: true })
		expect(run).not.toHaveBeenCalled()
		expect(event.defaultPrevented).toBe(false)
	})
})

describe('native menu', () => {
	it('runs the command bound to a menu item id', () => {
		const reg = registry({ nativeMenu: true })
		cleanup.push(reg.declare([cmd.shortcuts]))
		const show = vi.fn()
		cleanup.push(reg.register({ ...cmd.shortcuts, run: show }))
		expect(reg.runMenu('app_shortcuts')).toBe(true)
		expect(reg.runMenu('help_shortcuts')).toBe(true)
		expect(reg.runMenu('nope')).toBe(false)
		expect(show).toHaveBeenCalledTimes(2)
	})

	it('leaves a menu accelerator to the menu, so the chord runs once', () => {
		const reg = mounted(registry({ nativeMenu: true }))
		cleanup.push(reg.declare([cmd.closeTab, cmd.reset]))
		const close = vi.fn()
		const reset = vi.fn()
		cleanup.push(
			reg.register([
				{ ...cmd.closeTab, run: close },
				{ ...cmd.reset, run: reset },
			]),
		)
		// The webview sees the keydown, then the menu fires its accelerator.
		const w = press(document.body, { code: 'KeyW', metaKey: true })
		reg.runMenu('workspace_close_tab')
		expect(close).toHaveBeenCalledTimes(1)
		expect(w.defaultPrevented).toBe(false)
		// No menu item: keydown still owns it.
		press(document.body, { code: 'KeyR', metaKey: true, shiftKey: true })
		expect(reset).toHaveBeenCalledTimes(1)
	})

	it('uses keydown for menu commands in the browser preview', () => {
		const reg = mounted(registry({ nativeMenu: false }))
		cleanup.push(reg.declare([cmd.closeTab]))
		const close = vi.fn()
		cleanup.push(reg.register({ ...cmd.closeTab, run: close }))
		press(document.body, { code: 'KeyW', metaKey: true })
		expect(close).toHaveBeenCalledTimes(1)
	})

	it('runs from the menu whatever has focus', () => {
		const reg = registry({ nativeMenu: true, dialog: () => true })
		cleanup.push(reg.declare([cmd.save]))
		const save = vi.fn()
		cleanup.push(reg.register({ ...cmd.save, run: save }))
		reg.runMenu('workspace_save_query')
		expect(save).toHaveBeenCalledTimes(1)
	})
})

describe('keymap', () => {
	const menuRs = readFileSync(path.resolve(__dirname, '../../src-tauri/src/menu.rs'), 'utf8')
	/** menu id → accelerator (or null) for every `item(app, "id", "Title", enabled, accel)`. */
	const items = new Map<string, string | null>()
	for (const m of menuRs.matchAll(
		/item\(\s*app,\s*"([^"]+)",\s*"[^"]*",\s*[^,]+,\s*(None|Some\("([^"]+)"\))\s*,?\s*\)/g,
	)) {
		items.set(m[1]!, m[3] ?? null)
	}

	function fromAccelerator(accel: string): string {
		const parts = accel.split('+')
		const keyPart = parts.at(-1)!
		const mods = parts.slice(0, -1)
		const out: string[] = []
		if (mods.includes('CmdOrCtrl')) out.push('Mod')
		if (mods.includes('Shift')) out.push('Shift')
		return [...out, keyPart].join('+')
	}

	function normal(binding: string): string {
		const tokens = binding.split('+')
		const keyPart = tokens.at(-1)!
		const mods = tokens.slice(0, -1)
		const order = ['Mod', 'Shift']
		return [...order.filter((m) => mods.includes(m)), keyPart].join('+')
	}

	it('reads the menu items', () => {
		expect(items.get('workspace_save_query')).toBe('CmdOrCtrl+S')
		expect(items.get('home_connect')).toBeNull()
	})

	it('has unique ids', () => {
		const ids = KEYMAP.map((s) => s.id)
		expect(new Set(ids).size).toBe(ids.length)
	})

	it('names only menu items that exist, with the keys of their accelerator', () => {
		for (const spec of KEYMAP) {
			const menus = typeof spec.menu === 'string' ? [spec.menu] : (spec.menu ?? [])
			for (const id of menus) {
				expect(items.has(id), `${spec.id}: no menu item ${id}`).toBe(true)
				const accel = items.get(id)
				const keys = bindingsFor(spec.keys, true)
				if (accel) expect(keys.map(normal), spec.id).toEqual([fromAccelerator(accel)])
				else expect(keys, `${spec.id} has keys but ${id} has no accelerator`).toEqual([])
			}
		}
	})

	it('maps every frontend menu item to a command', () => {
		const native = new Set(['window_close'])
		const bound = new Set(
			KEYMAP.flatMap((s) => (typeof s.menu === 'string' ? [s.menu] : (s.menu ?? []))),
		)
		for (const id of items.keys()) {
			if (native.has(id)) continue
			expect(bound.has(id), `menu item ${id} runs no command`).toBe(true)
		}
	})

	it('keeps cmd K for quick open and leaves cmd P unbound', () => {
		const all = KEYMAP.flatMap((s) => bindingsFor(s.keys, true))
		expect(bindingsFor(cmd.quickOpen.keys, true)).toEqual(['Mod+K'])
		expect(all).not.toContain('Mod+P')
	})
})
