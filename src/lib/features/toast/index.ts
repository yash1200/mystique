/**
 * App-wide status toast. Import `showToast` from anywhere; mount `ToastContext` once.
 * Short notes that need no popup go to the status footer via `showStatus`.
 */
export {
	STATUS_NOTE_MS,
	type StatusNote as StatusNoteMessage,
	type StatusNoteTone,
} from './status-note'
export { appStatusNote, showStatus } from './status-note.svelte'
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
export { default as StatusNote } from './ui/StatusNote.svelte'
export { default as ToastContext } from './ui/ToastContext.svelte'
