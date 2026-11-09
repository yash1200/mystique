/** Query space: editor, results, saved queries, and history. */
import { lazy } from '#core/ui'

export * from './constants'
export * from './display'
export * from './history/history-api'
export type * from './history/history-types'
export * from './history/history.svelte'
export * from './saved/library.svelte'
export * from './policy'
export type * from './run/run-types'
export * from './saved/save.svelte'
export * from './saved/saved-api'
export type * from './saved/saved-types'
export { pruneEditorStates } from './editor/held'
export { prefetchQueryEditor, type QueryPrefetchTarget } from './prefetch'
/** The editor and CodeMirror load when a query tab first opens. */
export const QueryCanvas = lazy(() => import('./ui/QueryCanvas.svelte'))
export { default as QueryListPanel } from './ui/QueryListPanel.svelte'
export { default as QuerySaveHost } from './saved/QuerySaveHost.svelte'
export { default as SaveQueryModal } from './saved/SaveQueryModal.svelte'
