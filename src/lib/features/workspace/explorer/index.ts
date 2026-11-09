/** Object explorer and the structure panel. */
import { lazy } from '#core/ui'

export * from './actions'
export { default as ObjectExplorer } from './ui/ObjectExplorer.svelte'
/** The structure editor and diff load when a structure tab first opens. */
export const StructurePanel = lazy(() => import('./ui/StructurePanel.svelte'))
