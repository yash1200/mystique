/**
 * Workspace chrome that used to live on WorkspaceShell and was
 * drilled into SidePane / MainPane. Read this from services instead
 * of passing it as props.
 */
import type {
	ImportFormat,
	PreviewImportResult,
	TableMeta,
} from '#core/contracts'
export type StudioSideMode = 'explorer' | 'saved-queries' | 'history'

class WorkspaceStudio {
	sideMode = $state<StudioSideMode>('explorer')

	rowSyncTick = $state(0)
	tablePreviewTick = $state(0)
	baselineEpoch = $state(0)

	exportTable = $state<TableMeta | null>(null)
	exportBusy = $state(false)
	exportError = $state<string | null>(null)
	importTable = $state<TableMeta | null>(null)
	importBusy = $state(false)
	importError = $state<string | null>(null)
	importPath = $state('')
	importPreview = $state<PreviewImportResult | null>(null)
	importFormat = $state<ImportFormat>('csv')
}

export const workspaceStudio = new WorkspaceStudio()
