/**
 * Workspace chrome that used to live on WorkspaceShell and was
 * drilled into SidePane / MainPane. Read this from services instead
 * of passing it as props.
 */
import type {
	DatabaseMeta,
	ObjectStructure,
	PreviewImportResult,
	TableMeta,
	TablePreviewFilter,
	TablePreviewSort,
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
	tableLoading = $state(false)
	catalogError = $state<string | null>(null)

	sideMode = $state<StudioSideMode>('explorer')
	selectedQueryId = $state<string | null>(null)
	selectedHistoryId = $state<string | null>(null)
	queryHistory = $state<QueryHistoryEntry[]>([])
	historyLoading = $state(false)
	historyError = $state<string | null>(null)
	savedQueries = $state<SavedQueryListItem[]>([])
	querySaving = $state(false)

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
	tableSeedFilters = $state<TablePreviewFilter[] | null>(null)
	tableSeedCodeFilter = $state<string | null>(null)
	tableSeedSort = $state<TablePreviewSort | null>(null)

	exportTable = $state<TableMeta | null>(null)
	exportBusy = $state(false)
	exportError = $state<string | null>(null)
	importTable = $state<TableMeta | null>(null)
	importBusy = $state(false)
	importError = $state<string | null>(null)
	importPath = $state('')
	importPreview = $state<PreviewImportResult | null>(null)
}

export const workspaceStudio = new WorkspaceStudio()
