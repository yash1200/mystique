/**
 * Workspace chrome: which side panel is showing. Feature state lives in the
 * features' own per-connection stores (catalog, structure, er, query,
 * transfer), not here.
 */
export type StudioSideMode = 'explorer' | 'saved-queries' | 'history'

class WorkspaceStudio {
	sideMode = $state<StudioSideMode>('explorer')

	rowSyncTick = $state(0)
	tablePreviewTick = $state(0)
	baselineEpoch = $state(0)
}

export const workspaceStudio = new WorkspaceStudio()
