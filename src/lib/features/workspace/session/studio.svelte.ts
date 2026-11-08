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
import type {
	QueryHistoryEntry,
	SavedQueryListItem,
} from '#features/workspace/query'
export type StudioSideMode = 'explorer' | 'saved-queries' | 'history'

class WorkspaceStudio {
	sideMode = $state<StudioSideMode>('explorer')
	selectedQueryId = $state<string | null>(null)
	selectedHistoryId = $state<string | null>(null)
	queryHistory = $state<QueryHistoryEntry[]>([])
	historyLoading = $state(false)
	historyLoadingMore = $state(false)
	historyHasMore = $state(false)
	historyError = $state<string | null>(null)
	/** Connection id the current history list belongs to. */
	historyFor = $state<string | null>(null)
	savedQueries = $state<SavedQueryListItem[]>([])
	querySaving = $state(false)
	/** Connection id the current saved-query list belongs to. */
	savedQueriesFor = $state<string | null>(null)

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
