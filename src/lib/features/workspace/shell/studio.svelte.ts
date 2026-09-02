/**
 * Workspace chrome that used to live on WorkspaceShell and was
 * drilled into SidePane / MainPane. Read this from services instead
 * of passing it as props.
 */
import type {
	DatabaseMeta,
	ImportFormat,
	ObjectStructure,
	PreviewImportResult,
	TableMeta,
} from '$core/contracts'
import type { ErDiagramState } from '$features/workspace/er'
import type {
	QueryHistoryEntry,
	SavedQueryListItem,
} from '$features/workspace/query'
export type StudioSideMode = 'explorer' | 'saved-queries' | 'history'

export type LiveModel = {
	diagram: ErDiagramState
	title: string
	summary: string
	rev: number
	focusNodeId: string
}

class WorkspaceStudio {
	catalogs = $state<Record<string, DatabaseMeta[]>>({})
	selectedDb = $state<Record<string, string>>({})
	refreshing = $state(false)
	refreshingTables = $state(false)
	createBusy = $state(false)
	/** Tab whose preview is in flight. Per-tab so a stale finish cannot hide another open. */
	loadingTabId = $state<string | null>(null)
	catalogError = $state<string | null>(null)
	/** connectionId → last engineSessionId we loaded a live catalog for */
	liveCatalogKey = $state<Record<string, string>>({})
	catalogLoadInFlight = $state<Record<string, boolean>>({})

	sideMode = $state<StudioSideMode>('explorer')
	selectedQueryId = $state<string | null>(null)
	selectedHistoryId = $state<string | null>(null)
	queryHistory = $state<QueryHistoryEntry[]>([])
	historyLoading = $state(false)
	historyError = $state<string | null>(null)
	/** Connection id the current history list belongs to. */
	historyFor = $state<string | null>(null)
	savedQueries = $state<SavedQueryListItem[]>([])
	querySaving = $state(false)
	/** Connection id the current saved-query list belongs to. */
	savedQueriesFor = $state<string | null>(null)

	liveModels = $state<Record<string, LiveModel>>({})
	modelLoading = $state(false)
	modelError = $state<string | null>(null)

	liveStructures = $state<
		Record<string, { data: ObjectStructure; rev: number }>
	>({})
	structureLoading = $state(false)
	structureError = $state<string | null>(null)

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
