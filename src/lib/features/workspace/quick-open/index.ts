import { lazy } from '#core/ui'

export { registerQuickOpenCommands } from './bind'
export { quickOpen } from './chrome.svelte'
/** The palette's code loads the first time it opens. */
export const QuickOpen = lazy(() => import('./ui/QuickOpen.svelte'))
