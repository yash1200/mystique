/**
 * The updater behind a small interface, so the store can be tested with a
 * fake. The Rust commands (`src-tauri/src/ipc/updates.rs`) wrap the Tauri
 * updater with a connect timeout. Releases publish `latest.json`.
 */
import { Channel, invoke } from '#core/ipc'

export type UpdateInfo = {
	version: string
	currentVersion: string
	/** Release notes from `latest.json`. */
	notes: string | null
	date: string | null
}

/** An update found by a check. Download, then install; restart applies it. */
export type PendingUpdate = UpdateInfo & {
	download(onProgress: (received: number, total: number | null) => void): Promise<void>
	install(): Promise<void>
}

export type UpdaterBackend = {
	/** `null` when the app is current. */
	check(): Promise<PendingUpdate | null>
	relaunch(): Promise<void>
}

type DownloadEvent =
	| { event: 'Started'; data: { contentLength: number | null } }
	| { event: 'Progress'; data: { chunkLength: number } }
	| { event: 'Finished' }

export const tauriUpdater: UpdaterBackend = {
	async check() {
		const found = await invoke<UpdateInfo | null>('check_for_update')
		if (!found) return null
		return {
			...found,
			download(onProgress) {
				let received = 0
				let total: number | null = null
				const onEvent = new Channel<DownloadEvent>()
				onEvent.onmessage = (event) => {
					if (event.event === 'Started') total = event.data.contentLength ?? null
					else if (event.event === 'Progress') received += event.data.chunkLength
					else return
					onProgress(received, total)
				}
				return invoke('download_update', { onEvent })
			},
			install: () => invoke('install_update'),
		}
	},
	async relaunch() {
		const { relaunch } = await import('@tauri-apps/plugin-process')
		await relaunch()
	},
}

/** One sentence for a failed check or download. The raw error is logged. */
export function updateErrorMessage(err: unknown, during: 'check' | 'download'): string {
	const message =
		typeof err === 'string'
			? err
			: err && typeof err === 'object' && 'message' in err && typeof err.message === 'string'
				? err.message
				: JSON.stringify(err)
	const raw = (message ?? '').toLowerCase()
	if (raw.includes('signature')) {
		return 'The update failed its signature check, so it was not installed.'
	}
	if (
		['error sending request', 'dns', 'connect', 'timed out', 'timeout', 'offline', 'network'].some(
			(n) => raw.includes(n),
		)
	) {
		return 'Could not reach GitHub. Check your internet connection and try again.'
	}
	if (during === 'check' && (raw.includes('release json') || raw.includes('404'))) {
		return 'No update information is published for Mystique yet.'
	}
	return during === 'check'
		? 'Could not check for updates. Try again later.'
		: 'The update could not be downloaded. Try again later.'
}

/** The GitHub release page for a version. */
export function releaseUrl(version: string): string {
	return `https://github.com/yash1200/mystique/releases/tag/v${version}`
}
