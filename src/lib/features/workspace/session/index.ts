/**
 * Workspace kernel: sessions, tabs, drafts, live engine sessions, edit
 * buffers, the write-guard client, and close/reset guard state. No UI here;
 * every workspace feature may import it.
 */
export * from './api'
export * from './close-guard'
export * from './connection-cache.svelte'
export * from './edit-buffer'
export { guardCopy, typedNameOk, type GuardCopy } from './guard-copy'
export * from './live-session'
export * from './lookup'
export * from './reset'
export { resetGuard } from './reset-guard.svelte'
export { selectedDatabases } from './selected-db.svelte'
export * from './session-utils'
export * from './session.svelte'
export * from './studio.svelte'
export * from './tab-save'
export {
	askWriteGuard,
	currentWriteGuard,
	isWriteCancelled,
	resolveWriteGuard,
	runWithWriteGuard,
	subscribeWriteGuard,
	type GuardOptions,
} from './write-guard'
export type * from './types'
export { emptyTablePreview, resolveTablePreview } from './types'
