/** Native app menu (Tauri). No-ops in the browser preview. */
import { invoke } from '@tauri-apps/api/core'
import { isTauri } from './tauri'

export type NativeMenuPage = 'home' | 'workspace'

export type NativeMenuState = {
	page: NativeMenuPage
	hasSelection?: boolean
	hasSession?: boolean
	hasTab?: boolean
}

export function setNativeMenu(state: NativeMenuState): void {
	if (!isTauri()) return
	void invoke('set_native_menu', { state })
}

export async function listenNativeMenu(
	handlers: Record<string, () => void>,
): Promise<() => void> {
	if (!isTauri()) return () => {}
	const { listen } = await import('@tauri-apps/api/event')
	const unsubs = await Promise.all(
		Object.entries(handlers).map(([name, fn]) => listen(name, () => fn())),
	)
	return () => {
		for (const u of unsubs) u()
	}
}
