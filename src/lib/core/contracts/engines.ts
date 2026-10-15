/** Engine / session IPC types. Functions live in #core/ipc. */
import type { EngineCapabilities } from './engine-caps'

export interface PoolConfig {
	maxSize?: number
	minIdle?: number
	acquireTimeoutMs?: number
	idleTimeoutSecs?: number
}
export interface OpenSessionRequest {
	engine: string
	pluginConfig: Record<string, unknown>
	password?: string | null
	pool?: PoolConfig
	sessionId?: string | null
	/** When password is omitted, resolve the OS-keychain secret for this profile. */
	profileId?: string | null
	ssh?: SshTunnelRequest | null
	tlsKeyPassphrase?: string | null
	/**
	 * Test form sets this false so an unticked SSH box does not reuse the
	 * saved tunnel. Omitted means the saved profile's SSH config is used.
	 */
	useSavedSsh?: boolean
}

export type SshTunnelRequest = {
	host: string
	port: number
	user: string
	auth: 'password' | 'key' | 'agent'
	password?: string | null
	privateKeyPath?: string | null
	passphrase?: string | null
	trustedFingerprint?: string | null
}

export type SshHostKeyPrompt = {
	code: 'sshHostKey' | string
	kind: 'unknown' | 'mismatch' | string
	host: string
	port: number
	fingerprint: string
	keyType: string
	publicKey?: string
}

export type WriteGuardPrompt = {
	code: 'writeGuard' | string
	kind: 'unlock' | 'typeConfirm' | string
	environment: string
	connectionName: string
	preview: string
	objectName?: string | null
}
export interface EngineDescriptor {
	id: string
	displayName: string
	family: string
	queryLanguage: string
	available: boolean
	supportsPooling: boolean
	description: string
}
export interface PoolStatus {
	engine: string
	maxSize: number
	active: number
	idle: number
	size: number
}
export interface SessionInfo {
	sessionId: string
	engine: string
	displayName: string
	capabilities: EngineCapabilities
	pool?: PoolStatus | null
	openedAt: string
}
export interface TestResult {
	ok: boolean
	engine: string
	serverVersion?: string | null
	latencyMs: number
	message: string
	sshHostKey?: SshHostKeyPrompt | null
}
export interface ParsedConnectionUrl {
	engine: string
	pluginConfig: Record<string, unknown>
	password?: string | null
}
export interface ParseConnectionUrlRequest {
	engine: string
	url: string
}
export interface FormattedConnectionUrl {
	engine: string
	url: string
}
export interface FormatConnectionUrlRequest {
	engine: string
	pluginConfig: Record<string, unknown>
	password?: string | null
}
export type QueryMode = 'read' | 'write'

export interface ExecuteQueryRequest {
	sessionId: string
	text: string
	mode?: QueryMode
	maxRows?: number
	timeoutMs?: number
	profileReadOnly?: boolean
	confirmObjectName?: string | null
	/** MongoDB collection name */
	collection?: string | null
	/** MongoDB database override (explorer multi-DB) */
	database?: string | null
	/** Client id so Cancel / Esc / tab close can stop this run. */
	executionId?: string | null
}

export interface QueryColumn {
	name: string
	dataType?: string | null
}

export interface QueryResult {
	engine: string
	mode: QueryMode
	columns: QueryColumn[]
	rows: unknown[][]
	rowCount: number
	truncated: boolean
	durationMs: number
	message?: string | null
}

export interface SourcePosition {
	offset: number
	line: number
	column: number
}

export type StatementKind = 'read' | 'session' | 'write' | 'destructive'

export interface SqlRange {
	start: number
	end: number
}

/** One statement in a batch. `index` is 0-based. Extra sets share `index`. */
export interface StatementResult {
	index: number
	setIndex?: number
	sqlRange: SqlRange
	kind: StatementKind
	result?: QueryResult | null
	affectedRows?: number | null
	error?: string | null
	errorPosition?: SourcePosition | null
	notices?: string[]
}

export interface QueryBatchResult {
	results: StatementResult[]
	stoppedAt?: number | null
}

export type BatchPhase = 'started' | 'finished'

export interface BatchEvent {
	index: number
	total: number
	phase: BatchPhase
}

/** Plugin-owned column metadata (developer-facing types, enums, pk). */
export interface ColumnReference {
	database?: string | null
	schema?: string | null
	table: string
	column: string
}

export interface ColumnInfo {
	name: string
	dataType: string
	nullable: boolean
	pk?: boolean
	enumValues?: string[] | null
	defaultValue?: string | null
	/** Plugin-owned FK target for jump-to-row navigation. */
	references?: ColumnReference | null
}

export interface DescribeTableRequest {
	sessionId: string
	table: string
	schema?: string | null
	database?: string | null
}

export interface CatalogColumnRow {
	database?: string | null
	schema?: string | null
	table: string
	column: string
	dataType: string
	nullable: boolean
	pk?: boolean
}

export interface ListCatalogColumnsRequest {
	sessionId: string
	database?: string | null
	schema?: string | null
}

export interface TableEditColumn {
	name: string
	dataType?: string
	nullable?: boolean
	pk?: boolean
}

export interface TableRowUpdate {
	original: Record<string, unknown>
	values: Record<string, unknown>
}

export interface TableRowInsert {
	values: Record<string, unknown>
}

export interface TableRowDelete {
	original: Record<string, unknown>
}

export interface ApplyTableEditsRequest {
	sessionId: string
	table: string
	schema?: string | null
	database?: string | null
	columns: TableEditColumn[]
	updates: TableRowUpdate[]
	inserts: TableRowInsert[]
	deletes?: TableRowDelete[]
	profileReadOnly?: boolean
	confirmObjectName?: string | null
}

export interface ApplyTableEditsResult {
	updated: number
	inserted: number
	deleted?: number
}

export interface TablePreviewFilter {
	column: string
	op: string
	value: string
}

export type TablePreviewSortDir = 'asc' | 'desc'

export interface TablePreviewSort {
	column: string
	dir: TablePreviewSortDir
}

export interface FetchTablePreviewRequest {
	sessionId: string
	table: string
	schema?: string | null
	database?: string | null
	maxRows?: number
	offset?: number
	filters?: TablePreviewFilter[]
	/** Raw code-filter box — plugin compiles JSON or `col = val`. */
	codeFilter?: string | null
	/** Plugin-owned `ORDER BY` / Mongo `sort`. Omitted = default PK / `_id` / `rowid`. */
	sortColumn?: string | null
	sortDir?: TablePreviewSortDir | null
	/** Same id as an in-flight export on this tab. */
	executionId?: string | null
}

export interface TablePreviewResult {
	engine: string
	columns: ColumnInfo[]
	rows: unknown[][]
	rowCount: number
	totalRows?: number
	/** False until COUNT finishes. Pager must not treat totalRows as final. */
	totalKnown?: boolean
	truncated: boolean
	durationMs: number
}

export interface FetchTablePreviewStatsRequest {
	sessionId: string
	table: string
	schema?: string | null
	database?: string | null
	filters?: TablePreviewFilter[]
	codeFilter?: string | null
}

export interface TablePreviewColumnRef {
	column: string
	references: ColumnInfo['references']
}

export interface TablePreviewStats {
	totalRows: number
	totalKnown?: boolean
	references?: TablePreviewColumnRef[]
}

export interface CatalogObject {
	name: string
	schema?: string | null
	database?: string | null
	kind: string
	rowCount?: number
}

export interface CatalogDatabase {
	name: string
	meta?: string | null
	system?: boolean
	objects: CatalogObject[]
}

export interface ListCatalogRequest {
	sessionId: string
	preferredDatabase?: string | null
	/** false = database names only (no tables/collections). */
	includeObjects?: boolean
}

export interface CatalogRelationship {
	fromSchema?: string | null
	fromTable: string
	fromColumn: string
	toSchema?: string | null
	toTable: string
	toColumn: string
	name?: string | null
}

export interface ListRelationshipsRequest {
	sessionId: string
	database?: string | null
}

export type TableAdminOp =
	| 'createStatement'
	| 'createGroup'
	| 'createObject'
	| 'rename'
	| 'drop'
	| 'truncate'
	| 'duplicate'

export interface TableAdminRequest {
	sessionId: string
	op: TableAdminOp
	name: string
	schema?: string | null
	database?: string | null
	kind?: string | null
	newName?: string | null
	profileReadOnly?: boolean
	confirmObjectName?: string | null
}

export interface TableAdminResult {
	op: TableAdminOp
	statement?: string | null
	executed: boolean
	message?: string | null
}

/** Plugin-owned catalog admin (CREATE text + rename/drop/truncate/duplicate). */

export type ExportFormat = 'csv' | 'json' | 'jsonl' | 'sql'
export type TableFileFormat = ExportFormat

export interface ExportObjectRequest {
	sessionId: string
	name: string
	schema?: string | null
	database?: string | null
	kind?: string | null
	format: ExportFormat
	path: string
	/** Same id as the table preview, so closing the tab stops the file write. */
	executionId?: string | null
}

export interface ExportObjectResult {
	path: string
	rowCount: number
	format: ExportFormat
}

/** Plugin-owned full-object file export (100-row chunks, no row cap). */

export type ImportFormat = ExportFormat
export type ImportEncoding =
	| 'utf8'
	| 'utf16Le'
	| 'utf16Be'
	| 'windows1252'
	| 'iso88591'
export type ImportOnDuplicate = 'stopOnError' | 'updateOnDuplicate'

export interface ImportColumnMap {
	fileKey: string
	tableColumn?: string | null
}

export interface ImportTableColumn {
	name: string
	dataType: string
	pk: boolean
	nullable: boolean
}

export interface PreviewImportRequest {
	sessionId: string
	name: string
	schema?: string | null
	database?: string | null
	kind?: string | null
	path: string
	format: ImportFormat
	encoding?: ImportEncoding
}

export interface PreviewImportResult {
	fileKeys: string[]
	columns: ImportTableColumn[]
	suggestedMap: ImportColumnMap[]
	rows: unknown[][]
	format: ImportFormat
	encoding: ImportEncoding
}

export interface ImportObjectRequest {
	sessionId: string
	name: string
	schema?: string | null
	database?: string | null
	kind?: string | null
	path: string
	format: ImportFormat
	encoding?: ImportEncoding
	mapping: ImportColumnMap[]
	onDuplicate: ImportOnDuplicate
	profileReadOnly?: boolean
}

export interface ImportObjectResult {
	inserted: number
	updated: number
	format: ImportFormat
	message?: string | null
}

/** One structure section (columns / indexes / …) — plugin-owned. */
export interface StructureSection {
	id: string
	title: string
	description?: string | null
	columns: string[]
	rows: string[][]
	emptyMessage?: string | null
}

export interface ObjectStructure {
	title: string
	subtitle?: string | null
	objectKind?: string | null
	sections: StructureSection[]
}

export interface ObjectStructureRequest {
	sessionId: string
	table: string
	schema?: string | null
	database?: string | null
	kind?: string | null
}

/** Plugin-owned object structure for “View structure”. */

export type MetricKind =
	| 'count'
	| 'bytes'
	| 'durationMs'
	| 'percent'
	| 'text'
	| 'ratio'

export interface MetricValue {
	key: string
	label: string
	kind: MetricKind
	value?: number | null
	display?: string | null
	unit?: string | null
	max?: number | null
	available: boolean
	unavailableReason?: string | null
}

export interface ServerMetricsCapabilities {
	overview: boolean
	activeSessions: boolean
	terminateSession: boolean
	slowQueries: boolean
	notes: string[]
}

export interface ServerOverview {
	connections: MetricValue
	cacheHitRatio: MetricValue
	databaseSize: MetricValue
	uptime: MetricValue
	activeQueries: MetricValue
	extras: MetricValue[]
}

export interface ActiveBackendSession {
	id: string
	user?: string | null
	database?: string | null
	state?: string | null
	durationMs?: number | null
	query?: string | null
	canTerminate: boolean
	extras: MetricValue[]
}

export interface SlowQuery {
	id: string
	query: string
	durationMs?: number | null
	meanMs?: number | null
	calls?: number | null
	database?: string | null
	user?: string | null
	source?: string | null
	extras: MetricValue[]
}

export interface ServerMetricsSnapshot {
	engine: string
	collectedAt: string
	capabilities: ServerMetricsCapabilities
	overview: ServerOverview
	sessions: ActiveBackendSession[]
	slowQueries: SlowQuery[]
}

export interface FetchServerMetricsRequest {
	sessionId: string
	includeSessions?: boolean
	includeSlowQueries?: boolean
	slowQueryLimit?: number
	minDurationMs?: number
}

export interface TerminateSessionRequest {
	sessionId: string
	backendSessionId: string
	profileReadOnly?: boolean
}

export interface TerminateSessionResult {
	ok: boolean
	backendSessionId: string
	message?: string | null
}
