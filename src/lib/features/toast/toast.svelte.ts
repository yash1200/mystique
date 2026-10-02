/** App-wide status toast. One message at a time; auto-dismisses. */

export type ToastTone = 'info' | 'success' | 'warning' | 'danger'

export type ToastMessage = {
	kind: ToastTone
	message: string
}

/** Visible duration. Enter/exit motion is `DURATION.toast` in `#core/util`. */
const DISMISS_MS = 3600

class AppToast {
	current = $state<ToastMessage | null>(null)
	#timer: ReturnType<typeof setTimeout> | null = null

	show = (kind: ToastTone, message: string) => {
		this.current = { kind, message }
		this.#clearTimer()
		this.#timer = setTimeout(() => {
			this.current = null
			this.#timer = null
		}, DISMISS_MS)
	}

	#clearTimer() {
		if (!this.#timer) return
		clearTimeout(this.#timer)
		this.#timer = null
	}
}

export const appToast = new AppToast()

export function showToast(kind: ToastTone, message: string) {
	appToast.show(kind, message)
}
