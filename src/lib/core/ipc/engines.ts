/**
 * Engine / connection-pool API (Tauri IPC).
 * Mirrors mystique-core ConnectionManager surface.
 */
import { invoke } from '@tauri-apps/api/core'
import type {
	ApplyTableEditsRequest,
	ApplyTableEditsResult,
	CatalogColumnRow,
	CatalogDatabase,
	CatalogRelationship,
	ExecuteQueryRequest,
	ExportObjectRequest,
	ExportObjectResult,
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
	QueryResult,
	ServerMetricsSnapshot,
	SessionInfo,
	TableAdminRequest,
	TableAdminResult,
	TablePreviewResult,
	TablePreviewStats,
	TerminateSessionRequest,
	TerminateSessionResult,
	SshHostKeyPrompt,
	TestResult,
} from '../contracts/engines'

import { isTauri } from './tauri'
function requireTauri() {
	if (!isTauri()) {
		throw new Error('Engine pool API requires Tauri (`npm run tauri dev`).')
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

export async function executeEngineQuery(
	request: ExecuteQueryRequest,
): Promise<QueryResult> {
	requireTauri()
	return invoke('execute_engine_query', { request })
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
