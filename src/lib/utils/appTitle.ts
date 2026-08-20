import { isTauri } from '$lib/api/tauri'

/** Product name shown in the OS / browser window title bar. */
export const APP_TITLE = 'Mystique DB'

/** Monotonic token so overlapping async setTitle calls don't clobber a newer title. */
let titleGen = 0

/**
 * Build the window title.
 * Home: "Mystique DB"
 * Connected: "Mystique DB · {connectionName}"
 */
export function formatAppTitle(connectionName?: string | null): string {
	const name = connectionName?.trim()
	return name ? `${APP_TITLE} · ${name}` : APP_TITLE
}

/** Update the OS/browser window title only (no in-app chrome). */
export async function setAppTitle(
	connectionName?: string | null,
): Promise<string> {
	const title = formatAppTitle(connectionName)
	const gen = ++titleGen
	if (typeof document !== 'undefined') {
		document.title = title
	}
	if (!isTauri()) return title
	try {
		// Prefer webview window API (Tauri 2); fall back to window API.
		try {
			const { getCurrentWebviewWindow } =
				await import('@tauri-apps/api/webviewWindow')
			if (gen !== titleGen) return title
			await getCurrentWebviewWindow().setTitle(title)
			return title
		} catch {
			const { getCurrentWindow } = await import('@tauri-apps/api/window')
			if (gen !== titleGen) return title
			await getCurrentWindow().setTitle(title)
		}
	} catch {
		/* permission / non-window — document.title still applies */
	}
	return title
}
