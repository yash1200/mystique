/**
 * App updates: a quiet check at launch, a manual check from the menu and
 * Settings, a background download, then a prompt to restart.
 *
 * Installing right after the download is safe on macOS: the running app
 * keeps its code in memory, and the next launch is the new version, so
 * "Later" still updates on the next start.
 */
import { showToast } from '#features/toast'
import {
	tauriUpdater,
	updateErrorMessage,
	type PendingUpdate,
	type UpdateInfo,
	type UpdaterBackend,
} from './backend'

export type UpdatePhase =
	| { kind: 'idle' }
	| { kind: 'checking' }
	| { kind: 'current' }
	| { kind: 'available'; update: UpdateInfo }
	| { kind: 'downloading'; update: UpdateInfo; received: number; total: number | null }
	| { kind: 'ready'; update: UpdateInfo }
	/** `update` is set when the download failed, so it can be retried. */
	| { kind: 'failed'; message: string; update: UpdateInfo | null }

/** Wait after launch so the check never competes with startup work. */
export const LAUNCH_CHECK_DELAY_MS = 4000

type Notify = (kind: 'success' | 'danger', message: string) => void

function infoOf(update: PendingUpdate): UpdateInfo {
	const { version, currentVersion, notes, date } = update
	return { version, currentVersion, notes, date }
}

export class Updater {
	phase = $state<UpdatePhase>({ kind: 'idle' })
	/** The bottom-right card. Closing it never cancels a download. */
	noticeOpen = $state(false)
	checkedAt = $state<number | null>(null)

	#pending: PendingUpdate | null = null
	#backend: UpdaterBackend
	#notify: Notify

	constructor(backend: UpdaterBackend = tauriUpdater, notify: Notify = showToast) {
		this.#backend = backend
		this.#notify = notify
	}

	get busy(): boolean {
		return this.phase.kind === 'checking' || this.phase.kind === 'downloading'
	}

	/**
	 * Look for a newer release. A launch check is silent unless one exists;
	 * a manual check always answers.
	 */
	check = async ({ manual = false }: { manual?: boolean } = {}): Promise<void> => {
		const kind = this.phase.kind
		if (kind === 'checking') return
		if (kind === 'downloading' || kind === 'ready') {
			if (manual) this.noticeOpen = true
			return
		}
		this.phase = { kind: 'checking' }
		try {
			const update = await this.#backend.check()
			this.checkedAt = Date.now()
			this.#pending = update
			if (!update) {
				this.phase = { kind: 'current' }
				if (manual) this.#notify('success', 'Mystique is up to date.')
				return
			}
			this.phase = { kind: 'available', update: infoOf(update) }
			this.noticeOpen = true
		} catch (err) {
			console.warn('update check failed', err)
			this.#pending = null
			const message = updateErrorMessage(err, 'check')
			this.phase = { kind: 'failed', message, update: null }
			if (manual) this.#notify('danger', message)
		}
	}

	/** Download in the background, install, then ask to restart. */
	update = async (): Promise<void> => {
		const pending = this.#pending
		const kind = this.phase.kind
		if (!pending || (kind !== 'available' && kind !== 'failed')) return
		const update = infoOf(pending)
		this.phase = { kind: 'downloading', update, received: 0, total: null }
		let shownPercent = -1
		try {
			await pending.download((received, total) => {
				// One state change per percent, not per network chunk.
				const percent = total ? Math.floor((received / total) * 100) : -1
				if (percent === shownPercent && total) return
				shownPercent = percent
				this.phase = { kind: 'downloading', update, received, total }
			})
			await pending.install()
			this.phase = { kind: 'ready', update }
			this.noticeOpen = true
		} catch (err) {
			console.warn('update download failed', err)
			this.phase = { kind: 'failed', message: updateErrorMessage(err, 'download'), update }
			this.noticeOpen = true
		}
	}

	restart = async (): Promise<void> => {
		try {
			await this.#backend.relaunch()
		} catch (err) {
			console.warn('relaunch failed', err)
			this.#notify('danger', 'Could not restart Mystique. Quit and open it again to finish updating.')
		}
	}

	dismiss = (): void => {
		this.noticeOpen = false
	}

	/** The launch check. Skipped in `tauri dev`, which has no bundle to replace. */
	start = (): (() => void) => {
		if (import.meta.env.DEV) return () => {}
		const id = setTimeout(() => void this.check(), LAUNCH_CHECK_DELAY_MS)
		return () => clearTimeout(id)
	}
}

export const updater = new Updater()
