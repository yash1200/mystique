/** App-wide preferences and the command keymap. A leaf: features read it, it reads nothing back. */
export * from './chrome.svelte'
export * from './prefs.svelte'
export * from './query-timeout'
export type * from './prefs-types'
export { parseReviewBeforeSaving } from './prefs-types'
export * from './commands'
