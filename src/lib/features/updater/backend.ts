/**
 * The Tauri updater behind a small interface, so the store can be tested
 * with a fake. Releases publish `latest.json` (see docs/RELEASING.md).
 */

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

const CHECK_TIMEOUT_MS = 30_000

export const tauriUpdater: UpdaterBackend = {
	async check() {
		const { check } = await import('@tauri-apps/plugin-updater')
		const update = await check({ timeout: CHECK_TIMEOUT_MS })
		if (!update) return null
		return {
			version: update.version,
			currentVersion: update.currentVersion,
			notes: update.body?.trim() || null,
			date: update.date ?? null,
			download(onProgress) {
				let received = 0
				let total: number | null = null
				return update.download((event) => {
					if (event.event === 'Started') total = event.data.contentLength ?? null
					else if (event.event === 'Progress') received += event.data.chunkLength
					onProgress(received, total)
				})
			},
			install: () => update.install(),
		}
	},
	async relaunch() {
		const { relaunch } = await import('@tauri-apps/plugin-process')
		await relaunch()
	},
}

/** One sentence for a failed check or download. The raw error is logged. */
export function updateErrorMessage(err: unknown, during: 'check' | 'download'): string {
	const raw = (
		err instanceof Error ? err.message : typeof err === 'string' ? err : JSON.stringify(err)
	).toLowerCase()
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
