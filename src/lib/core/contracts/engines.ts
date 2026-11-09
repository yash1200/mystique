/**
 * Engine / session IPC types, generated from the Rust crates (`pnpm gen:ipc`).
 * Functions live in #core/ipc. Only frontend-only shapes are written here.
 */
import type {
	ExportFormat,
	TableFilter,
	WriteGuardPrompt as WriteGuardPromptWire,
} from '../ipc/bindings.gen'

export type {
	ActiveBackendSession,
	ApplyTableEditsRequest,
	ApplyTableEditsResult,
	BatchEvent,
	BatchPhase,
	CatalogColumnRow,
	CatalogDatabase,
	CatalogObject,
	CatalogRelationship,
	ColumnInfo,
	DescribeTableRequest,
	DistinctColumnValuesRequest,
	ExecuteQueryRequest,
	ExplainPlan,
	ExplainQueryRequest,
	ExportFormat,
	ExportObjectRequest,
	ExportObjectResult,
	FetchLargeValueRequest,
	FetchLargeValueResult,
	FetchServerMetricsRequest,
	FetchTablePreviewRequest,
	FetchTablePreviewStatsRequest,
	FormatConnectionUrlRequest,
	FormattedConnectionUrl,
	ImportColumnMap,
	ImportEncoding,
	ImportFormat,
	ImportObjectRequest,
	ImportObjectResult,
	ImportOnDuplicate,
	ImportTableColumn,
	LargeCellKey,
	LargeCellLocator,
	ListCatalogColumnsRequest,
	ListCatalogRequest,
	ListRelationshipsRequest,
	MetricKind,
	MetricValue,
	ObjectStructure,
	ObjectStructureRequest,
	OpenSessionRequest,
	ParseConnectionUrlRequest,
	ParsedConnectionUrl,
	PlanField,
	PlanNode,
	PoolConfig,
	PoolStatus,
	PreviewImportRequest,
	PreviewImportResult,
	PreviewWriteRowsRequest,
	QueryBatchResult,
	QueryColumn,
	QueryMode,
	QueryResult,
	RollbackTableEditsRequest,
	ServerMetricsCapabilities,
	ServerMetricsSnapshot,
	ServerOverview,
	SessionInfo,
	SlowQuery,
	SourcePosition,
	SqlRange,
	SshHostKeyPrompt,
	SshTunnelRequest,
	StatementKind,
	StatementResult,
	StructureSection,
	TableAdminOp,
	TableAdminRequest,
	TableAdminResult,
	TableEditColumn,
	TablePreviewColumnRef,
	TablePreviewResult,
	TablePreviewStats,
	TablePreviewCount,
	TableCountOutcome,
	TableRowDelete,
	TableRowInsert,
	TableRowUpdate,
	TerminateSessionRequest,
	TerminateSessionResult,
	TimedUnlockStatus,
	TransferProgress,
	TestResult,
	WriteRowPreview,
} from '../ipc/bindings.gen'

/** One grid filter clause (`TableFilter` on the wire). */
export type TablePreviewFilter = TableFilter

export type TablePreviewSortDir = 'asc' | 'desc'

export interface TablePreviewSort {
	column: string
	dir: TablePreviewSortDir
}

/** Formats a table file can be exported to or imported from. */
export type TableFileFormat = ExportFormat

/**
 * The write guard dialog. `kind` is `unlock`, `confirm`, `typeConfirm`, or
 * `review` (a save the frontend asked to show first). `title` and
 * `confirmLabel` are set by the frontend only.
 */
export type WriteGuardPrompt = WriteGuardPromptWire & {
	title?: string
	confirmLabel?: string
	/** The engine session the write runs on; enables the row preview. */
	sessionId?: string
}
