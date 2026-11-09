/** ER / model diagram: canvas, layout, DBML snapshot, PNG export. */
import { lazy } from '#core/ui'

export * from './api'
export * from './constants'
export * from './context-menu'
export * from './dbml'
export * from './export-png'
export * from './geometry'
export * from './layout'
export * from './live'
export * from './store.svelte'
export type * from './types'
/** The diagram UI loads when a model tab first opens. */
export const ErWorkspace = lazy(() => import('./ui/ErWorkspace.svelte'))
