/**
 * App-wide status toast. Import `showToast` from anywhere; mount `ToastContext` once.
 * Short notes that need no popup go to the status footer via `showStatus`.
 */
export { STATUS_NOTE_MS } from './status-note'
export { showStatus } from './status-note.svelte'
export { ACTION_DISMISS_MS, toastLifetime } from './toast-policy'
export { showToast } from './toast.svelte'
export { default as StatusNote } from './ui/StatusNote.svelte'
export { default as ToastContext } from './ui/ToastContext.svelte'
