/** App-wide status toast. Import `showToast` from anywhere; mount `ToastContext` once. */
export {
	appToast,
	clearToast,
	showToast,
	type ToastMessage,
	type ToastTone,
} from './toast.svelte'
export { default as ToastContext } from './ui/ToastContext.svelte'
