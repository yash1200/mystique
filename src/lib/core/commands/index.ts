/**
 * App commands: one registry for keys, the native menu and code.
 *
 * Features register handlers while mounted (`commands.register` returns the
 * unregister). The keymap — ids, titles, bindings, menu ids — is declared once
 * at startup, so the shortcuts view lists exactly what the keys do.
 */
import { listenMenuCommands } from '../ipc/menu'
import { openDialogCount } from '../ui/dialog-stack'
import { isMacPlatform } from '../util/platform'
import { createCommandRegistry } from './registry'

export * from './keys'
export * from './registry'
export type { Focus } from './scope'

export const commands = createCommandRegistry({
	mac: isMacPlatform,
	dialogOpen: () => openDialogCount() > 0,
})

/** Install the keydown dispatcher and the native-menu listener. */
export function mountCommands(): () => void {
	const unmountKeys = commands.mount(window)
	let unlisten = () => {}
	let cancelled = false
	void listenMenuCommands((id) => commands.runMenu(id))
		.then((u) => {
			if (cancelled) u()
			else unlisten = u
		})
		.catch(() => {
			/* no native menu */
		})
	return () => {
		cancelled = true
		unmountKeys()
		unlisten()
	}
}
