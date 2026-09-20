/**
 * DTOs shared with the Rust side. These mirror `mystique-connector-api` and the
 * app-DB rows — they are not ours to reshape. Everything else lives next to the
 * feature that owns it.
 *
 * Name clashes resolved here:
 * - `EngineFamily` is the plugin capability enum (`RELATIONAL` | …).
 *   The picker label type is `ConnectionFamily` (`relational` | `document`).
 * - `ColumnReference` is the catalog FK target; the engines IPC clone is not
 *   re-exported.
 */
export type * from './catalog'
export type * from './connection'
export type * from './engine-caps'
export type {
	ActiveBackendSession,
	ApplyTableEditsRequest,
	ApplyTableEditsResult,
	CatalogColumnRow,
	CatalogDatabase,
	CatalogObject,
	CatalogRelationship,
	ColumnInfo,
	DescribeTableRequest,
	EngineDescriptor,
	ExecuteQueryRequest,
	ExportFormat,
	ExportObjectRequest,
	ExportObjectResult,
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
	PoolConfig,
	PoolStatus,
	PreviewImportRequest,
	PreviewImportResult,
	QueryColumn,
	QueryMode,
	QueryResult,
	ServerMetricsCapabilities,
	ServerMetricsSnapshot,
	ServerOverview,
	SessionInfo,
	SlowQuery,
	StructureSection,
	TableAdminOp,
	TableAdminRequest,
	TableAdminResult,
	TableEditColumn,
	TableFileFormat,
	TablePreviewColumnRef,
	TablePreviewFilter,
	TablePreviewResult,
	TablePreviewSort,
	TablePreviewSortDir,
	TablePreviewStats,
	TableRowDelete,
	TableRowInsert,
	TableRowUpdate,
	TerminateSessionRequest,
	TerminateSessionResult,
	SshHostKeyPrompt,
	SshTunnelRequest,
	TestResult,
} from './engines'
export type * from './session'
