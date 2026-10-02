/** App-wide About dialog. Open from Mystique → About (or Help on Windows). */
import { isTauri } from '#core/ipc'

class AppAboutUi {
	open = $state(false)
	version = $state<string | null>(null)

	show() {
		this.open = true
		void this.#ensureVersion()
	}

	close() {
		this.open = false
	}

	async #ensureVersion() {
		if (this.version) return
		if (isTauri()) {
			try {
				const { getVersion } = await import('@tauri-apps/api/app')
				this.version = await getVersion()
				return
			} catch {
				/* browser-shaped fallback */
			}
		}
		this.version = '1.0.0'
	}
}

export const appAboutUi = new AppAboutUi()
