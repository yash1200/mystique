/**
 * Quit guard. Closing the window or quitting the app while an export or
 * import runs is held by the backend, which emits `quit-requested` with the
 * number of transfers running. The window asks, then calls `confirmQuit`.
 */
import { invoke } from './invoke'

const QUIT_REQUESTED = 'quit-requested'

export async function listenQuitRequests(
	onRequest: (running: number) => void,
): Promise<() => void> {
	const { listen } = await import('@tauri-apps/api/event')
	return listen<number>(QUIT_REQUESTED, (event) => onRequest(event.payload))
}

/** Cancel running transfers (partial exports are removed) and quit. */
export async function confirmQuit(): Promise<void> {
	return invoke('confirm_quit')
}
