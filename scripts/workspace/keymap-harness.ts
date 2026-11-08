/** Test helper: the app keymap on a fresh registry, with every command recorded. */
import { KEYMAP } from '../../src/lib/app/commands'
import { createCommandRegistry } from '../../src/lib/core/commands/registry'

export type Harness = {
	/** Press on `target` (default: body); returns the ids that ran. */
	press: (
		init: KeyboardEventInit & { code?: string; key?: string },
		target?: EventTarget,
	) => string[]
	ran: string[]
	runMenu: (menuId: string) => string[]
	dispose: () => void
}

export function keymapHarness(opts: {
	mac: boolean
	nativeMenu: boolean
	dialogOpen?: () => boolean
	/** Only these ids get a handler (default: all). */
	only?: string[]
}): Harness {
	const reg = createCommandRegistry({
		mac: () => opts.mac,
		nativeMenu: () => opts.nativeMenu,
		dialogOpen: opts.dialogOpen ?? (() => false),
	})
	const ran: string[] = []
	const undeclare = reg.declare(KEYMAP)
	const specs = opts.only ? KEYMAP.filter((s) => opts.only!.includes(s.id)) : KEYMAP
	const unregister = reg.register(specs.map((s) => ({ ...s, run: () => void ran.push(s.id) })))
	const unmount = reg.mount(window)
	return {
		ran,
		press(init, target = document.body) {
			const before = ran.length
			target.dispatchEvent(
				new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init }),
			)
			return ran.slice(before)
		},
		runMenu(menuId) {
			const before = ran.length
			reg.runMenu(menuId)
			return ran.slice(before)
		},
		dispose() {
			unmount()
			unregister()
			undeclare()
		},
	}
}
