/** Toast timing. Plain TS so tests can import it. */

export type ToastTone = 'info' | 'success' | 'warning' | 'danger'

export type ToastAction = {
	label: string
	run: () => void | Promise<void>
}

export type ToastOptions = {
	/** One button, e.g. Undo. Clicking it runs the action and dismisses. */
	action?: ToastAction
	/** Stay until dismissed. Errors are sticky unless this is `false`. */
	sticky?: boolean
}

/** Visible duration. Enter/exit motion is `DURATION.toast` in `#core/util`. */
const DISMISS_MS = 3600
/** A toast with an action stays long enough to reach the button. */
export const ACTION_DISMISS_MS = 8000

/** How long a toast stays, or `null` for until dismissed. */
export function toastLifetime(
	kind: ToastTone,
	options: ToastOptions = {},
): number | null {
	const sticky = options.sticky ?? kind === 'danger'
	if (sticky) return null
	return options.action ? ACTION_DISMISS_MS : DISMISS_MS
}
