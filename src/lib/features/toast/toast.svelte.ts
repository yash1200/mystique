/** App-wide status toast. One message at a time. */

import {
	toastLifetime,
	type ToastAction,
	type ToastOptions,
	type ToastTone,
} from './toast-policy'

export type ToastMessage = {
	id: number
	kind: ToastTone
	message: string
	action?: ToastAction
	sticky: boolean
}

class AppToast {
	current = $state<ToastMessage | null>(null)
	#timer: ReturnType<typeof setTimeout> | null = null
	#seq = 0

	show = (kind: ToastTone, message: string, options: ToastOptions = {}) => {
		const lifetime = toastLifetime(kind, options)
		this.current = {
			id: ++this.#seq,
			kind,
			message,
			action: options.action,
			sticky: lifetime === null,
		}
		this.#clearTimer()
		if (lifetime !== null) {
			this.#timer = setTimeout(this.dismiss, lifetime)
		}
	}

	dismiss = () => {
		this.#clearTimer()
		this.current = null
	}

	runAction = async () => {
		const action = this.current?.action
		this.dismiss()
		await action?.run()
	}

	#clearTimer() {
		if (!this.#timer) return
		clearTimeout(this.#timer)
		this.#timer = null
	}
}

export const appToast = new AppToast()

export function showToast(
	kind: ToastTone,
	message: string,
	options?: ToastOptions,
) {
	appToast.show(kind, message, options)
}

export function dismissToast() {
	appToast.dismiss()
}
