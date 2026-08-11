/**
 * Engine / connection-pool API (Tauri IPC).
 * Mirrors mystique-core ConnectionManager surface.
 */
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "./tauri";

export interface PoolConfig {
  maxSize?: number;
  minIdle?: number;
  acquireTimeoutMs?: number;
  idleTimeoutSecs?: number;
}

export interface OpenSessionRequest {
  engine: string;
  pluginConfig: Record<string, unknown>;
  password?: string | null;
  pool?: PoolConfig;
  sessionId?: string | null;
}

export interface EngineDescriptor {
  id: string;
  displayName: string;
  family: string;
  queryLanguage: string;
  available: boolean;
  supportsPooling: boolean;
  description: string;
}

export interface PoolStatus {
  engine: string;
  maxSize: number;
  active: number;
  idle: number;
  size: number;
}

export interface SessionInfo {
  sessionId: string;
  engine: string;
  displayName: string;
  capabilities: import("$lib/types/engineCaps").EngineCapabilities;
  pool?: PoolStatus | null;
  openedAt: string;
}

export interface TestResult {
  ok: boolean;
  engine: string;
  serverVersion?: string | null;
  latencyMs: number;
  message: string;
}

function requireTauri() {
  if (!isTauri()) {
    throw new Error("Engine pool API requires Tauri (`npm run tauri dev`).");
  }
}

export async function listEngines(): Promise<EngineDescriptor[]> {
  requireTauri();
  return invoke("list_engines");
}

export async function testEngineConnection(
  request: OpenSessionRequest,
): Promise<TestResult> {
  requireTauri();
  return invoke("test_engine_connection", { request });
}

export async function openEngineSession(
  request: OpenSessionRequest,
): Promise<SessionInfo> {
  requireTauri();
  return invoke("open_engine_session", { request });
}

export async function closeEngineSession(sessionId: string): Promise<void> {
  requireTauri();
  return invoke("close_engine_session", { sessionId });
}

export async function pingEngineSession(sessionId: string): Promise<void> {
  requireTauri();
  return invoke("ping_engine_session", { sessionId });
}

export async function engineSessionPoolStatus(
  sessionId: string,
): Promise<PoolStatus | null> {
  requireTauri();
  return invoke("engine_session_pool_status", { sessionId });
}

export async function listEngineSessions(): Promise<SessionInfo[]> {
  requireTauri();
  return invoke("list_engine_sessions");
}

export async function getEngineSession(
  sessionId: string,
): Promise<SessionInfo | null> {
  requireTauri();
  return invoke("get_engine_session", { sessionId });
}

export type QueryMode = "read" | "write";

export interface ExecuteQueryRequest {
  sessionId: string;
  text: string;
  mode?: QueryMode;
  maxRows?: number;
  timeoutMs?: number;
  profileReadOnly?: boolean;
  /** MongoDB collection name */
  collection?: string | null;
  /** MongoDB database override (explorer multi-DB) */
  database?: string | null;
}

export interface QueryColumn {
  name: string;
  dataType?: string | null;
}

export interface QueryResult {
  engine: string;
  mode: QueryMode;
  columns: QueryColumn[];
  rows: unknown[][];
  rowCount: number;
  truncated: boolean;
  durationMs: number;
  message?: string | null;
}

export async function executeEngineQuery(
  request: ExecuteQueryRequest,
): Promise<QueryResult> {
  requireTauri();
  return invoke("execute_engine_query", { request });
}

/** Plugin-owned column metadata (developer-facing types, enums, pk). */
export interface ColumnInfo {
  name: string;
  dataType: string;
  nullable: boolean;
  pk?: boolean;
  enumValues?: string[] | null;
}

export interface DescribeTableRequest {
  sessionId: string;
  table: string;
  schema?: string | null;
  database?: string | null;
}

export interface CatalogColumnRow {
  database?: string | null;
  schema?: string | null;
  table: string;
  column: string;
  dataType: string;
  nullable: boolean;
  pk?: boolean;
}

export interface ListCatalogColumnsRequest {
  sessionId: string;
  database?: string | null;
  schema?: string | null;
}

export async function describeEngineTable(
  request: DescribeTableRequest,
): Promise<ColumnInfo[]> {
  requireTauri();
  return invoke("describe_engine_table", { request });
}

export async function listEngineCatalogColumns(
  request: ListCatalogColumnsRequest,
): Promise<CatalogColumnRow[]> {
  requireTauri();
  return invoke("list_engine_catalog_columns", { request });
}

export interface TableEditColumn {
  name: string;
  dataType?: string;
  nullable?: boolean;
  pk?: boolean;
}

export interface TableRowUpdate {
  original: Record<string, unknown>;
  values: Record<string, unknown>;
}

export interface TableRowInsert {
  values: Record<string, unknown>;
}

export interface TableRowDelete {
  original: Record<string, unknown>;
}

export interface ApplyTableEditsRequest {
  sessionId: string;
  table: string;
  schema?: string | null;
  database?: string | null;
  columns: TableEditColumn[];
  updates: TableRowUpdate[];
  inserts: TableRowInsert[];
  deletes?: TableRowDelete[];
  profileReadOnly?: boolean;
}

export interface ApplyTableEditsResult {
  updated: number;
  inserted: number;
  deleted?: number;
}

export async function applyEngineTableEdits(
  request: ApplyTableEditsRequest,
): Promise<ApplyTableEditsResult> {
  requireTauri();
  return invoke("apply_engine_table_edits", { request });
}

export interface TablePreviewFilter {
  column: string;
  op: string;
  value: string;
}

export interface FetchTablePreviewRequest {
  sessionId: string;
  table: string;
  schema?: string | null;
  database?: string | null;
  maxRows?: number;
  offset?: number;
  filters?: TablePreviewFilter[];
}

export interface TablePreviewResult {
  engine: string;
  columns: ColumnInfo[];
  rows: unknown[][];
  rowCount: number;
  totalRows?: number;
  truncated: boolean;
  durationMs: number;
}

export async function fetchEngineTablePreview(
  request: FetchTablePreviewRequest,
): Promise<TablePreviewResult> {
  requireTauri();
  return invoke("fetch_engine_table_preview", { request });
}

export interface CatalogObject {
  name: string;
  schema?: string | null;
  database?: string | null;
  kind: string;
  rowCount?: number;
}

export interface CatalogDatabase {
  name: string;
  meta?: string | null;
  system?: boolean;
  objects: CatalogObject[];
}

export interface ListCatalogRequest {
  sessionId: string;
  preferredDatabase?: string | null;
}

export async function listEngineCatalog(
  request: ListCatalogRequest,
): Promise<CatalogDatabase[]> {
  requireTauri();
  return invoke("list_engine_catalog", { request });
}

export interface CatalogRelationship {
  fromSchema?: string | null;
  fromTable: string;
  fromColumn: string;
  toSchema?: string | null;
  toTable: string;
  toColumn: string;
  name?: string | null;
}

export interface ListRelationshipsRequest {
  sessionId: string;
  database?: string | null;
}

export async function listEngineRelationships(
  request: ListRelationshipsRequest,
): Promise<CatalogRelationship[]> {
  requireTauri();
  return invoke("list_engine_relationships", { request });
}

export type TableAdminOp =
  | "createStatement"
  | "rename"
  | "drop"
  | "truncate"
  | "duplicate";

export interface TableAdminRequest {
  sessionId: string;
  op: TableAdminOp;
  name: string;
  schema?: string | null;
  database?: string | null;
  kind?: string | null;
  newName?: string | null;
  profileReadOnly?: boolean;
}

export interface TableAdminResult {
  op: TableAdminOp;
  statement?: string | null;
  executed: boolean;
  message?: string | null;
}

/** Plugin-owned catalog admin (CREATE text + rename/drop/truncate/duplicate). */
export async function applyEngineTableAdmin(
  request: TableAdminRequest,
): Promise<TableAdminResult> {
  requireTauri();
  return invoke("apply_engine_table_admin", { request });
}
