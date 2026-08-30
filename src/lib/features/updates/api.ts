import { isTauri } from '$core/ipc'
import type { DownloadEvent, Update } from '@tauri-apps/plugin-updater'

export type UpdateInfo = {
	version: string
	notes: string | null
}

let pending: Update | null = null

export function clearPendingUpdate() {
	if (pending) {
		void pending.close().catch(() => {})
		pending = null
	}
}

export async function checkForUpdate(): Promise<UpdateInfo | null> {
	if (!isTauri()) return null
	clearPendingUpdate()
	const { check } = await import('@tauri-apps/plugin-updater')
	const found = await check()
	if (!found) return null
	pending = found
	return {
		version: found.version,
		notes: found.body?.trim() || null,
	}
}

export async function installPendingUpdate(
	onEvent?: (event: DownloadEvent) => void,
): Promise<void> {
	if (!pending) throw new Error('No update ready to install.')
	await pending.downloadAndInstall(onEvent)
	const { relaunch } = await import('@tauri-apps/plugin-process')
	await relaunch()
}
