/** Product name shown in the OS / browser window title bar. */
const APP_TITLE = 'Mystique'

/** The current window title, for the custom titlebar (the OS one is hidden). */
export const appTitle = $state({ current: APP_TITLE })

/** Monotonic token so overlapping async setTitle calls don't clobber a newer title. */
let titleGen = 0

/**
 * Build the window title.
 * Home: "Mystique"
 * Connected: "Mystique · {connectionName}"
 */
function formatAppTitle(connectionName?: string | null): string {
	const name = connectionName?.trim()
	return name ? `${APP_TITLE} · ${name}` : APP_TITLE
}

/** Update the window title: the OS one and the custom titlebar's. */
export async function setAppTitle(
	connectionName?: string | null,
): Promise<string> {
	const title = formatAppTitle(connectionName)
	const gen = ++titleGen
	appTitle.current = title
	if (typeof document !== 'undefined') {
		document.title = title
	}
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
