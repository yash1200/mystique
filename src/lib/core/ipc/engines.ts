/**
 * Engine / connection-pool API (Tauri IPC).
 * Mirrors mystique-core ConnectionManager surface.
 */
import { Channel, invoke } from '@tauri-apps/api/core'
import type {
	ApplyTableEditsRequest,
	ApplyTableEditsResult,
	DistinctColumnValuesRequest,
	RollbackTableEditsRequest,
	BatchEvent,
	CatalogColumnRow,
	CatalogDatabase,
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
	ListCatalogRequest,
	ListRelationshipsRequest,
	ObjectStructure,
	ObjectStructureRequest,
	OpenSessionRequest,
	ParseConnectionUrlRequest,
	ParsedConnectionUrl,
	PreviewImportRequest,
	PreviewImportResult,
	QueryBatchResult,
	QueryResult,
	ServerMetricsSnapshot,
	SessionInfo,
	SshHostKeyPrompt,
	TableAdminRequest,
	TableAdminResult,
	TablePreviewResult,
	TablePreviewStats,
	TerminateSessionRequest,
	TerminateSessionResult,
	TestResult,
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

import { isTauri } from './tauri'
function requireTauri() {
	if (!isTauri()) {
		throw new Error('Engine pool API requires the Mystique desktop app.')
	}
}

/** Parse a connection URL via the engine plugin (backend-backed). */
export async function parseEngineConnectionUrl(
	request: ParseConnectionUrlRequest,
): Promise<ParsedConnectionUrl> {
	requireTauri()
	return invoke('parse_engine_connection_url', { request })
}
/** Format discrete plugin config into a connection URL (plugin-owned). */
export async function formatEngineConnectionUrl(
	request: FormatConnectionUrlRequest,
): Promise<FormattedConnectionUrl> {
	requireTauri()
	return invoke('format_engine_connection_url', { request })
}

export async function testEngineConnection(
	request: OpenSessionRequest,
): Promise<TestResult> {
	requireTauri()
	return invoke('test_engine_connection', { request })
}

export async function trustSshHostKey(prompt: SshHostKeyPrompt): Promise<void> {
	requireTauri()
	return invoke('trust_ssh_host_key', { prompt })
}

export async function openEngineSession(
	request: OpenSessionRequest,
): Promise<SessionInfo> {
	requireTauri()
	return invoke('open_engine_session', { request })
}

/** Reuse a live session (e.g. opened by MCP) when the id already exists. */
export async function ensureEngineSession(
	request: OpenSessionRequest,
): Promise<SessionInfo> {
	requireTauri()
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
	requireTauri()
	return invoke('close_engine_session', { sessionId })
}

export async function pingEngineSession(sessionId: string): Promise<void> {
	requireTauri()
	return invoke('ping_engine_session', { sessionId })
}

export async function listEngineSessions(): Promise<SessionInfo[]> {
	requireTauri()
	return invoke('list_engine_sessions')
}

export async function getEngineSession(
	sessionId: string,
): Promise<SessionInfo | null> {
	requireTauri()
	return invoke('get_engine_session', { sessionId })
}

export async function unlockEngineWrites(sessionId: string): Promise<void> {
	requireTauri()
	return invoke('unlock_engine_writes', { sessionId })
}

export async function executeEngineQuery(
	request: ExecuteQueryRequest,
): Promise<QueryResult> {
	requireTauri()
	return invoke('execute_engine_query', { request })
}

/** Run every statement. `onProgress` fires at statement boundaries, not per row. */
export interface PinStatus {
	pinned: boolean
	inTransaction: boolean
	/** An open transaction was discarded (unpin, idle timeout, disconnect). */
	rolledBack?: boolean
}

export async function pinQueryTab(
	sessionId: string,
	tabId: string,
): Promise<PinStatus> {
	requireTauri()
	return invoke('pin_query_tab', { sessionId, tabId })
}

export async function unpinQueryTab(tabId: string): Promise<PinStatus> {
	requireTauri()
	return invoke('unpin_query_tab', { tabId })
}

export async function commitPinnedTab(tabId: string): Promise<PinStatus> {
	requireTauri()
	return invoke('commit_pinned_tab', { tabId })
}

export async function rollbackPinnedTab(tabId: string): Promise<PinStatus> {
	requireTauri()
	return invoke('rollback_pinned_tab', { tabId })
}

export async function pinnedTabStatus(tabId: string): Promise<PinStatus> {
	requireTauri()
	return invoke('pinned_tab_status', { tabId })
}

export async function executePinnedBatch(
	request: ExecuteQueryRequest,
	tabId: string,
	onProgress?: (event: BatchEvent) => void,
): Promise<QueryBatchResult> {
	requireTauri()
	const channel = new Channel<BatchEvent>()
	if (onProgress) channel.onmessage = onProgress
	return invoke('execute_pinned_batch', { request, tabId, onProgress: channel })
}

export async function explainEngineQuery(
	request: ExplainQueryRequest,
): Promise<ExplainPlan> {
	requireTauri()
	return invoke('explain_engine_query', { request })
}

export async function executeEngineBatch(
	request: ExecuteQueryRequest,
	onProgress?: (event: BatchEvent) => void,
): Promise<QueryBatchResult> {
	requireTauri()
	const channel = new Channel<BatchEvent>()
	if (onProgress) channel.onmessage = onProgress
	return invoke('execute_engine_batch', { request, onProgress: channel })
}

export async function cancelEngineQuery(executionId: string): Promise<void> {
	requireTauri()
	return invoke('cancel_engine_query', { executionId })
}
export async function listEngineCatalogColumns(
	request: ListCatalogColumnsRequest,
): Promise<CatalogColumnRow[]> {
	requireTauri()
	return invoke('list_engine_catalog_columns', { request })
}

export async function applyEngineTableEdits(
	request: ApplyTableEditsRequest,
): Promise<ApplyTableEditsResult> {
	requireTauri()
	return invoke('apply_engine_table_edits', { request })
}

export async function rollbackEngineTableEdits(
	request: RollbackTableEditsRequest,
): Promise<void> {
	requireTauri()
	return invoke('rollback_engine_table_edits', { request })
}

export async function distinctEngineColumnValues(
	request: DistinctColumnValuesRequest,
): Promise<unknown[]> {
	requireTauri()
	return invoke('distinct_engine_column_values', { request })
}

export async function fetchEngineLargeValue(
	request: FetchLargeValueRequest,
): Promise<FetchLargeValueResult> {
	requireTauri()
	return invoke('fetch_engine_large_value', { request })
}

export async function fetchEngineTablePreview(
	request: FetchTablePreviewRequest,
): Promise<TablePreviewResult> {
	requireTauri()
	return invoke('fetch_engine_table_preview', { request })
}

export async function fetchEngineTablePreviewStats(
	request: FetchTablePreviewStatsRequest,
): Promise<TablePreviewStats> {
	requireTauri()
	return invoke('fetch_engine_table_preview_stats', { request })
}

export async function listEngineCatalog(
	request: ListCatalogRequest,
): Promise<CatalogDatabase[]> {
	requireTauri()
	return invoke('list_engine_catalog', { request })
}

export async function listEngineRelationships(
	request: ListRelationshipsRequest,
): Promise<CatalogRelationship[]> {
	requireTauri()
	return invoke('list_engine_relationships', { request })
}

/** Plugin-owned catalog admin (CREATE text + rename/drop/truncate/duplicate). */
export async function applyEngineTableAdmin(
	request: TableAdminRequest,
): Promise<TableAdminResult> {
	requireTauri()
	return invoke('apply_engine_table_admin', { request })
}

/** Plugin-owned full-object file export (100-row chunks, no row cap). */
export async function exportEngineObject(
	request: ExportObjectRequest,
): Promise<ExportObjectResult> {
	requireTauri()
	return invoke('export_engine_object', { request })
}

export async function previewEngineImport(
	request: PreviewImportRequest,
): Promise<PreviewImportResult> {
	requireTauri()
	return invoke('preview_engine_import', { request })
}

export async function importEngineObject(
	request: ImportObjectRequest,
): Promise<ImportObjectResult> {
	requireTauri()
	return invoke('import_engine_object', { request })
}

export async function describeEngineTableDefinition(
	request: DescribeTableDefinitionRequest,
): Promise<TableDefinition> {
	requireTauri()
	return invoke('describe_engine_table_definition', { request })
}

export async function listEngineTableDefinitions(
	request: ListTableDefinitionsRequest,
): Promise<TableDefinition[]> {
	requireTauri()
	return invoke('list_engine_table_definitions', { request })
}

export async function planEngineDdl(request: PlanDdlRequest): Promise<DdlPlan> {
	requireTauri()
	return invoke('plan_engine_ddl', { request })
}

export async function applyEngineDdl(
	request: ApplyDdlRequest,
): Promise<ApplyDdlResult> {
	requireTauri()
	return invoke('apply_engine_ddl', { request })
}

export async function planEngineSchemaDiff(
	request: PlanSchemaDiffRequest,
): Promise<SchemaMigration> {
	requireTauri()
	return invoke('plan_engine_schema_diff', { request })
}

export async function applyEngineSchemaDiff(
	request: ApplySchemaDiffRequest,
): Promise<ApplyDdlResult> {
	requireTauri()
	return invoke('apply_engine_schema_diff', { request })
}

/** Plugin-owned object structure for “View structure”. */
export async function describeEngineObjectStructure(
	request: ObjectStructureRequest,
): Promise<ObjectStructure> {
	requireTauri()
	return invoke('describe_engine_object_structure', { request })
}

export async function fetchEngineServerMetrics(
	request: FetchServerMetricsRequest,
): Promise<ServerMetricsSnapshot> {
	requireTauri()
	return invoke('fetch_engine_server_metrics', { request })
}

export async function terminateEngineSession(
	request: TerminateSessionRequest,
): Promise<TerminateSessionResult> {
	requireTauri()
	return invoke('terminate_engine_session', { request })
}
