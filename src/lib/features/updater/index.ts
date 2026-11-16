/**
 * App updates from GitHub Releases. Mount `UpdateNotice` once (the
 * bottom-right corner) and call `updater.start()` at launch.
 */
export type { UpdateInfo } from './backend'
export { updater, type UpdatePhase } from './updater.svelte'
export { default as UpdateNotice } from './ui/UpdateNotice.svelte'
export { default as UpdateSettingsRow } from './ui/UpdateSettingsRow.svelte'
