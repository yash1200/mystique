/** Links and the version for the About dialog. */
import { invoke } from '#core/ipc'

export const REPO_URL = 'https://github.com/yash1200/mystique'

export function releaseNotesUrl(version: string): string {
	return `${REPO_URL}/releases/tag/v${version}`
}

export const LICENSE_URL = `${REPO_URL}/blob/main/LICENSE`

/** The app version; `null` outside the desktop shell. */
export async function readAppVersion(): Promise<string | null> {
	try {
		const { getVersion } = await import('@tauri-apps/api/app')
		return await getVersion()
	} catch {
		return null
	}
}

/** Open a web page in the default browser (the opener plugin's default scope). */
export async function openExternal(url: string): Promise<void> {
	try {
		await invoke('plugin:opener|open_url', { url })
	} catch {
		window.open(url, '_blank', 'noopener')
	}
}
