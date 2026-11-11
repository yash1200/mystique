/**
 * Engine / connection-pool API (Tauri IPC).
 * Mirrors mystique-core ConnectionManager surface.
 */
import { Channel, invoke } from './invoke'
import type {
	ApplyTableEditsRequest,
	ApplyTableEditsResult,
	DistinctColumnValuesRequest,
	RollbackTableEditsRequest,
	BatchEvent,
	CatalogColumnRow,
	CatalogDatabase,
	CatalogPage,
	CatalogRelationship,
	ExecuteQueryRequest,
	ExplainPlan,
	ExplainQueryRequest,
	ExportObjectRequest,
	ExportObjectResult,
	FetchLargeValueRequest,
	FetchLargeValueResult,
	FetchServerMetricsRequest,
	FetchTablePreviewRequest,
	FetchTablePreviewStatsRequest,
	FormatConnectionUrlRequest,
	FormattedConnectionUrl,
	ImportObjectRequest,
	ImportObjectResult,
	ListCatalogColumnsRequest,
	ListCatalogChildrenRequest,
	ListCatalogRequest,
	ListRelationshipsRequest,
	ObjectStructure,
	ObjectStructureRequest,
	OpenSessionRequest,
	ParseConnectionUrlRequest,
	ParsedConnectionUrl,
	PreviewImportRequest,
	PreviewImportResult,
	PreviewWriteRowsRequest,
	QueryBatchResult,
	ServerMetricsSnapshot,
	SessionInfo,
	SshHostKeyPrompt,
	TableAdminRequest,
	TableAdminResult,
	TablePreviewCount,
	TablePreviewResult,
	TerminateSessionRequest,
	TerminateSessionResult,
	TestResult,
	TimedUnlockStatus,
	TransferProgress,
	WriteRowPreview,
} from '../contracts/engines'
import type {
	ApplyDdlRequest,
	ApplyDdlResult,
	ApplySchemaDiffRequest,
	DescribeTableDefinitionRequest,
	DdlPlan,
	ListTableDefinitionsRequest,
	PlanDdlRequest,
	PlanSchemaDiffRequest,
	SchemaMigration,
	TableDefinition,
} from '../contracts/ddl'
import type { PinStatus } from '../contracts/ipc'

export type { PinStatus }

/** Parse a connection URL via the engine plugin (backend-backed). */
export async function parseEngineConnectionUrl(
	request: ParseConnectionUrlRequest,
): Promise<ParsedConnectionUrl> {
	return invoke('parse_engine_connection_url', { request })
}
/** Format discrete plugin config into a connection URL (plugin-owned). */
export async function formatEngineConnectionUrl(
	request: FormatConnectionUrlRequest,
): Promise<FormattedConnectionUrl> {
	return invoke('format_engine_connection_url', { request })
}

export async function testEngineConnection(
	request: OpenSessionRequest,
): Promise<TestResult> {
	return invoke('test_engine_connection', { request })
}

export async function trustSshHostKey(prompt: SshHostKeyPrompt): Promise<void> {
	return invoke('trust_ssh_host_key', { prompt })
}

export async function openEngineSession(
	request: OpenSessionRequest,
): Promise<SessionInfo> {
	return invoke('open_engine_session', { request })
}

/** Reuse a live session (e.g. opened by MCP) when the id already exists. */
export async function ensureEngineSession(
	request: OpenSessionRequest,
): Promise<SessionInfo> {
	const sid = request.sessionId?.trim()
	if (sid) {
		try {
			const existing = await getEngineSession(sid)
			if (existing) {
				await pingEngineSession(sid)
				return existing
			}
		} catch {
			/* open a fresh pool */
		}
	}
	return openEngineSession(request)
}

export async function closeEngineSession(sessionId: string): Promise<void> {
	return invoke('close_engine_session', { sessionId })
}

export async function pingEngineSession(sessionId: string): Promise<void> {
	return invoke('ping_engine_session', { sessionId })
}

export async function listEngineSessions(): Promise<SessionInfo[]> {
	return invoke('list_engine_sessions')
}

export async function getEngineSession(
	sessionId: string,
): Promise<SessionInfo | null> {
	return invoke('get_engine_session', { sessionId })
}

export async function unlockEngineWrites(sessionId: string): Promise<void> {
	return invoke('unlock_engine_writes', { sessionId })
}

/** Allow writes on a read-only connection for 60 or 300 seconds. */
export async function startTimedWriteUnlock(
	sessionId: string,
	seconds: number,
): Promise<TimedUnlockStatus> {
	return invoke('start_timed_write_unlock', { sessionId, seconds })
}

export async function endTimedWriteUnlock(sessionId: string): Promise<TimedUnlockStatus> {
	return invoke('end_timed_write_unlock', { sessionId })
}

export async function timedWriteUnlockStatus(
	sessionId: string,
): Promise<TimedUnlockStatus> {
	return invoke('timed_write_unlock_status', { sessionId })
}

/** The rows one write would touch. Runs reads only. */
export async function previewWriteRows(
	request: PreviewWriteRowsRequest,
): Promise<WriteRowPreview> {
	return invoke('preview_write_rows', { request })
}

export async function pinQueryTab(
	sessionId: string,
	tabId: string,
): Promise<PinStatus> {
	return invoke('pin_query_tab', { sessionId, tabId })
}

export async function unpinQueryTab(tabId: string): Promise<PinStatus> {
	return invoke('unpin_query_tab', { tabId })
}

export async function commitPinnedTab(tabId: string): Promise<PinStatus> {
	return invoke('commit_pinned_tab', { tabId })
}

export async function rollbackPinnedTab(tabId: string): Promise<PinStatus> {
	return invoke('rollback_pinned_tab', { tabId })
}

export async function pinnedTabStatus(tabId: string): Promise<PinStatus> {
	return invoke('pinned_tab_status', { tabId })
}

export async function executePinnedBatch(
	request: ExecuteQueryRequest,
	tabId: string,
	onProgress?: (event: BatchEvent) => void,
): Promise<QueryBatchResult> {
	const channel = new Channel<BatchEvent>()
	if (onProgress) channel.onmessage = onProgress
	return invoke('execute_pinned_batch', { request, tabId, onProgress: channel })
}

export async function explainEngineQuery(
	request: ExplainQueryRequest,
): Promise<ExplainPlan> {
	return invoke('explain_engine_query', { request })
}

export async function executeEngineBatch(
	request: ExecuteQueryRequest,
	onProgress?: (event: BatchEvent) => void,
): Promise<QueryBatchResult> {
	const channel = new Channel<BatchEvent>()
	if (onProgress) channel.onmessage = onProgress
	return invoke('execute_engine_batch', { request, onProgress: channel })
}

export async function cancelEngineQuery(executionId: string): Promise<void> {
	return invoke('cancel_engine_query', { executionId })
}
export async function listEngineCatalogColumns(
	request: ListCatalogColumnsRequest,
): Promise<CatalogColumnRow[]> {
	return invoke('list_engine_catalog_columns', { request })
}

export async function applyEngineTableEdits(
	request: ApplyTableEditsRequest,
): Promise<ApplyTableEditsResult> {
	return invoke('apply_engine_table_edits', { request })
}

export async function rollbackEngineTableEdits(
	request: RollbackTableEditsRequest,
): Promise<void> {
	return invoke('rollback_engine_table_edits', { request })
}

export async function distinctEngineColumnValues(
	request: DistinctColumnValuesRequest,
): Promise<unknown[]> {
	return invoke('distinct_engine_column_values', { request })
}

export async function fetchEngineLargeValue(
	request: FetchLargeValueRequest,
): Promise<FetchLargeValueResult> {
	return invoke('fetch_engine_large_value', { request })
}

export async function fetchEngineTablePreview(
	request: FetchTablePreviewRequest,
): Promise<TablePreviewResult> {
	return invoke('fetch_engine_table_preview', { request })
}

export async function fetchEngineTablePreviewStats(
	request: FetchTablePreviewStatsRequest,
): Promise<TablePreviewCount> {
	return invoke('fetch_engine_table_preview_stats', { request })
}

export async function listEngineCatalog(
	request: ListCatalogRequest,
): Promise<CatalogDatabase[]> {
	return invoke('list_engine_catalog', { request })
}

/** One explorer level, a page at a time. Follow `nextCursor` for more. */
export async function listEngineCatalogChildren(
	request: ListCatalogChildrenRequest,
): Promise<CatalogPage> {
	return invoke('list_engine_catalog_children', { request })
}

export async function listEngineRelationships(
	request: ListRelationshipsRequest,
): Promise<CatalogRelationship[]> {
	return invoke('list_engine_relationships', { request })
}

/** Plugin-owned catalog admin (CREATE text + rename/drop/truncate/duplicate). */
export async function applyEngineTableAdmin(
	request: TableAdminRequest,
): Promise<TableAdminResult> {
	return invoke('apply_engine_table_admin', { request })
}

/** Plugin-owned full-object file export (100-row chunks, no row cap). */
/** Streams the table to a file. `onRows` gets the rows written so far. */
export async function exportEngineObject(
	request: ExportObjectRequest,
	onRows?: (rows: number) => void,
): Promise<ExportObjectResult> {
	const channel = new Channel<TransferProgress>()
	if (onRows) channel.onmessage = (event) => onRows(event.rows)
	return invoke('export_engine_object', { request, onProgress: channel })
}

/** Exports and imports still running on the backend (the quit guard asks). */
export async function runningEngineTransfers(): Promise<number> {
	return invoke('running_engine_transfers')
}

/** Explorer refresh: the backend re-reads catalog metadata next time. */
export async function invalidateEngineMetadata(
	sessionId: string,
	database?: string | null,
): Promise<void> {
	return invoke('invalidate_engine_metadata', { sessionId, database: database ?? null })
}

export async function previewEngineImport(
	request: PreviewImportRequest,
): Promise<PreviewImportResult> {
	return invoke('preview_engine_import', { request })
}

/** Imports the file batch by batch. `onRows` gets the rows written so far. */
export async function importEngineObject(
	request: ImportObjectRequest,
	onRows?: (rows: number) => void,
): Promise<ImportObjectResult> {
	const channel = new Channel<TransferProgress>()
	if (onRows) channel.onmessage = (event) => onRows(event.rows)
	return invoke('import_engine_object', { request, onProgress: channel })
}

export async function describeEngineTableDefinition(
	request: DescribeTableDefinitionRequest,
): Promise<TableDefinition> {
	return invoke('describe_engine_table_definition', { request })
}

export async function listEngineTableDefinitions(
	request: ListTableDefinitionsRequest,
): Promise<TableDefinition[]> {
	return invoke('list_engine_table_definitions', { request })
}

export async function planEngineDdl(request: PlanDdlRequest): Promise<DdlPlan> {
	return invoke('plan_engine_ddl', { request })
}

export async function applyEngineDdl(
	request: ApplyDdlRequest,
): Promise<ApplyDdlResult> {
	return invoke('apply_engine_ddl', { request })
}

export async function planEngineSchemaDiff(
	request: PlanSchemaDiffRequest,
): Promise<SchemaMigration> {
	return invoke('plan_engine_schema_diff', { request })
}

export async function applyEngineSchemaDiff(
	request: ApplySchemaDiffRequest,
): Promise<ApplyDdlResult> {
	return invoke('apply_engine_schema_diff', { request })
}

/** Plugin-owned object structure for “View structure”. */
export async function describeEngineObjectStructure(
	request: ObjectStructureRequest,
): Promise<ObjectStructure> {
	return invoke('describe_engine_object_structure', { request })
}

export async function fetchEngineServerMetrics(
	request: FetchServerMetricsRequest,
): Promise<ServerMetricsSnapshot> {
	return invoke('fetch_engine_server_metrics', { request })
}

export async function terminateEngineSession(
	request: TerminateSessionRequest,
): Promise<TerminateSessionResult> {
	return invoke('terminate_engine_session', { request })
}
