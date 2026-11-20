/** The app menu: native on macOS, drawn by the custom titlebar elsewhere. */
import { invoke } from '@tauri-apps/api/core'
import { isMacPlatform } from '../util/platform'

export type NativeMenuPage = 'home' | 'workspace'

export type NativeMenuState = {
	page: NativeMenuPage
	hasSelection?: boolean
	hasSession?: boolean
	hasTab?: boolean
	hasClosedTab?: boolean
	/** Home: the new / edit connection form is on screen (enables Save). */
	hasForm?: boolean
}

/** What the current page enables in the menu. The titlebar menu reads it. */
export const menuState = $state<{ current: NativeMenuState }>({
	current: { page: 'home' },
})

export function setNativeMenu(state: NativeMenuState): void {
	menuState.current = state
	if (isMacPlatform()) void invoke('set_native_menu', { state })
}

/** The one event menu.rs emits for a chosen item; the payload is the item id. */
const MENU_COMMAND_EVENT = 'menu-command'

/** Call `onItem` with the id of each native menu item the user picks. */
export async function listenMenuCommands(
	onItem: (menuId: string) => void,
): Promise<() => void> {
	const { listen } = await import('@tauri-apps/api/event')
	return listen<string>(MENU_COMMAND_EVENT, (event) => onItem(event.payload))
}
