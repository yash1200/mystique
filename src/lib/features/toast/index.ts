/** App-wide status toast. Import `showToast` from anywhere; mount `ToastContext` once. */
export {
	ACTION_DISMISS_MS,
	toastLifetime,
	type ToastAction,
	type ToastOptions,
	type ToastTone,
} from './toast-policy'
export {
	appToast,
	dismissToast,
	showToast,
	type ToastMessage,
} from './toast.svelte'
export { default as ToastContext } from './ui/ToastContext.svelte'
