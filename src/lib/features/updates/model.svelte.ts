import {
	checkForUpdate,
	clearPendingUpdate,
	installPendingUpdate,
	type UpdateInfo,
} from './api'

class AppUpdates {
	available = $state<UpdateInfo | null>(null)
	dismissed = $state(false)
	busy = $state(false)
	error = $state<string | null>(null)
	downloaded = $state(0)
	total = $state<number | null>(null)

	get visible() {
		return Boolean(this.available) && !this.dismissed
	}

	get progressLabel() {
		if (!this.busy) return null
		if (this.total && this.total > 0) {
			const pct = Math.min(100, Math.round((this.downloaded / this.total) * 100))
			return `Downloading ${pct}%`
		}
		return 'Downloading…'
	}

	async checkOnLaunch() {
		this.dismissed = false
		this.error = null
		try {
			this.available = await checkForUpdate()
		} catch {
			this.available = null
		}
	}

	dismiss() {
		this.dismissed = true
		this.error = null
		clearPendingUpdate()
	}

	async install() {
		if (!this.available || this.busy) return
		this.busy = true
		this.error = null
		this.downloaded = 0
		this.total = null
		try {
			await installPendingUpdate((event) => {
				if (event.event === 'Started') {
					this.total = event.data.contentLength ?? null
					this.downloaded = 0
				} else if (event.event === 'Progress') {
					this.downloaded += event.data.chunkLength
				}
			})
		} catch (err) {
			this.error = err instanceof Error ? err.message : String(err)
			this.busy = false
		}
	}
}

export const appUpdates = new AppUpdates()
