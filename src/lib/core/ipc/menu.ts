/** Native app menu (Tauri). No-ops in the browser preview. */
import { invoke } from '@tauri-apps/api/core'
import { isTauri } from './tauri'

export type NativeMenuPage = 'home' | 'workspace'

export type NativeMenuState = {
	page: NativeMenuPage
	hasSelection?: boolean
	hasSession?: boolean
	hasTab?: boolean
	hasClosedTab?: boolean
}

export function setNativeMenu(state: NativeMenuState): void {
	if (!isTauri()) return
	void invoke('set_native_menu', { state })
}

/** The one event menu.rs emits for a chosen item; the payload is the item id. */
export const MENU_COMMAND_EVENT = 'menu-command'

/** Call `onItem` with the id of each native menu item the user picks. */
export async function listenMenuCommands(
	onItem: (menuId: string) => void,
): Promise<() => void> {
	if (!isTauri()) return () => {}
	const { listen } = await import('@tauri-apps/api/event')
	return listen<string>(MENU_COMMAND_EVENT, (event) => onItem(event.payload))
}
