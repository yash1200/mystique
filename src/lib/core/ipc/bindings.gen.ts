// generated, do not edit.
// Source: the Rust IPC types (ts-rs). Regenerate with `pnpm gen:ipc`.
/* eslint-disable */

/**
 * One live backend session / connection / operation.
 */
export type ActiveBackendSession = { 
/**
 * Engine-native id (`pid`, MySQL `Id`, Mongo `opid`).
 */
id: string, user: string | null, database: string | null, state: string | null, durationMs: number | null, query: string | null, canTerminate: boolean, extras: Array<MetricValue>, };

export type AgentChat = { id: string, connectionId: string, title: string, provider: string, model: string, createdAt: string, updatedAt: string, };

/**
 * Why a run ended without an answer.
 */
export type AgentErrorKind = "cancelled" | "timeout" | "provider" | "internal";

/**
 * One streamed event of an agent run.
 */
export type AgentEvent = { "type": "textDelta", text: string, } | { "type": "reasoningDelta", text: string, } | { "type": "toolCallStart", id: string, name: string, args: unknown, } | { "type": "toolCallEnd", id: string, ok: boolean, summary: string, 
/**
 * The tool card row in local SQLite.
 */
messageId?: string, } | { "type": "toolApprovalPending", id: string, approvalId: string, sql: string, 
/**
 * Access level the statement needs (`write` / `full`).
 */
needed: string, } | { "type": "contextCompacted", droppedMessages: number, trimmedResults: number, } | { "type": "exhausted", turns: number, } | { "type": "done", messageId: string, tokens: AgentTokens, } | { "type": "error", kind: AgentErrorKind, message: string, };

/**
 * "Fix with AI" from the query error banner: the failed statement, the
 * error, and where it points (1-based), for the repair prompt.
 */
export type AgentFixRequest = { sql: string, error: string, line?: number | null, column?: number | null, };

export type AgentMessage = { id: string, chatId: string, role: string, content: string, toolName: string | null, draftBody: string | null, createdAt: string, };

export type AgentModelInfo = { id: string, label: string, };

export type AgentProviderInfo = { id: string, label: string, needsKey: boolean, defaultModel: string, };

/**
 * A table the user referenced with `@` or dragged in from the explorer.
 */
export type AgentTableRef = { schema?: string | null, name: string, };

/**
 * Token counts the provider reported for the whole run (all model calls).
 */
export type AgentTokens = { input: number, output: number, 
/**
 * Input tokens served from the provider's prompt cache.
 */
cachedInput: number, };

export type AiKeyStatus = { provider: string, hasKey: boolean, };

/**
 * Settings > AI, as the `get_ai_settings` family of commands returns it.
 */
export type AiSettingsView = { enabled: boolean, ollamaBaseUrl: string, 
/**
 * True after the user saves an Ollama URL (even the default).
 */
ollamaConfigured: boolean, keys: Array<AiKeyStatus>, providers: Array<AgentProviderInfo>, };

export type AppendHistoryInput = { connectionId?: string | null, engine?: string | null, body: string, language?: string | null, success: boolean, rowCount?: number | null, durationMs?: number | null, errorMessage?: string | null, 
/**
 * Catalog name. A URI is dropped at insert.
 */
database?: string | null, affectedRows?: number | null, source?: QueryHistorySource, kind?: QueryHistoryKind, };

export type ApplyDdlRequest = { sessionId: string, draft: TableDefinition, confirmObjectName?: string | null, };

export type ApplyDdlResult = { applied: Array<string>, message: string, };

export type ApplySchemaDiffRequest = { sessionId: string, source: Array<TableDefinition>, target: Array<TableDefinition>, renames?: Array<RenamePair>, 
/**
 * Changes to skip and drops to include. Drops are off unless listed.
 */
selection?: DiffSelection, confirmObjectName?: string | null, };

/**
 * IPC request for a grid save. Read-only comes from the live session.
 */
export type ApplyTableEditsRequest = { sessionId: string, table: string, schema?: string | null, database?: string | null, columns?: Array<TableEditColumn>, updates?: Array<TableRowUpdate>, inserts?: Array<TableRowInsert>, deletes?: Array<TableRowDelete>, confirmObjectName?: string | null, 
/**
 * Compile the statements and do not write.
 */
dryRun?: boolean, };

export type ApplyTableEditsResult = { updated: number, inserted: number, deleted: number, 
/**
 * Literal SQL the confirm dialog shows. One statement per changed row.
 */
statements: Array<string>, 
/**
 * Backend id for rolling back this save. Set by the manager once the undo is stored.
 */
saveId?: string | null, };

/**
 * Statement-boundary progress. Not emitted once per row.
 */
export type BatchEvent = { 
/**
 * 0-based statement index.
 */
index: number, total: number, phase: BatchPhase, };

/**
 * Where a progress event sits in the batch.
 */
export type BatchPhase = "started" | "finished";

/**
 * Flat column row for ER / bulk catalog (plugin-normalized types).
 */
export type CatalogColumnRow = { database?: string | null, schema?: string | null, table: string, column: string, dataType: string, nullable: boolean, pk: boolean, };

/**
 * One database / catalog in the explorer.
 */
export type CatalogDatabase = { name: string, meta?: string | null, system: boolean, objects: Array<CatalogObject>, };

/**
 * One table / view / collection in the explorer.
 */
export type CatalogObject = { name: string, schema?: string | null, database?: string | null, 
/**
 * `table` | `view` | `collection`
 */
kind: string, rowCount: number, };

/**
 * Declared relationship (FK or equivalent) for live ER.
 */
export type CatalogRelationship = { fromSchema?: string | null, fromTable: string, fromColumn: string, toSchema?: string | null, toTable: string, toColumn: string, name?: string | null, };

export type ColumnDraft = { name: string, dataType: string, nullable: boolean, pkPosition?: number, original?: ColumnOriginal | null, drop?: boolean, 
/**
 * Default expression. `None` means no default.
 */
defaultValue?: string | null, 
/**
 * See [`ColumnOriginal::identity`].
 */
identity?: string | null, };

/**
 * One column as described by the engine plugin (developer-facing types).
 */
export type ColumnInfo = { name: string, 
/**
 * Engine type label as a developer would write it (`timestamptz`, `varchar(255)`).
 */
dataType: string, nullable: boolean, pk: boolean, 
/**
 * Enum / set member labels when the engine exposes them.
 */
enumValues?: Array<string> | null, 
/**
 * Default expression / value when the engine exposes it.
 */
defaultValue?: string | null, 
/**
 * When set, the grid may jump to this referenced table/column for a cell value.
 */
references?: ColumnReference | null, };

export type ColumnOriginal = { name: string, dataType: string, nullable: boolean, pkPosition?: number, 
/**
 * MySQL: the column definition after its name, as `SHOW CREATE TABLE`
 * prints it. MODIFY is rebuilt from it so DEFAULT, ON UPDATE,
 * AUTO_INCREMENT, COMMENT, charset, and collation survive.
 */
definition?: string | null, 
/**
 * Default expression as the catalog prints it.
 */
defaultValue?: string | null, 
/**
 * Identity or auto-increment: `always` / `by default` (Postgres),
 * `auto_increment` (MySQL), `autoincrement` (SQLite).
 */
identity?: string | null, };

/**
 * Foreign-key (or equivalent) target for a column — plugin-owned navigation.
 * The FE shows a jump control; it does not invent relationships.
 */
export type ColumnReference = { database?: string | null, schema?: string | null, table: string, column: string, };

/**
 * A column pair the diff treats as a rename.
 */
export type ColumnRename = { 
/**
 * Source-side name (the name to end with).
 */
from: string, 
/**
 * Target-side name (the name it has now).
 */
to: string, 
/**
 * Paired by the heuristic, not by the user.
 */
guessed: boolean, };

/**
 * How Home collects / displays connection coordinates.
 */
export type ConnectionKind = "host" | "uri" | "file";

/**
 * Saved connection profile (any engine).
 *
 * List/get never include a password. Secrets are vault envelopes in the
 * `secrets` table; `secret_ref` is [`VAULT_SECRET_REF`] for those. Older rows
 * still name a keychain account until their first use moves it.
 */
export type ConnectionProfile = { id: string, name: string, 
/**
 * Plugin id: `postgres`, `mongodb`, `redis`, …
 */
engine: string, colorId: string, environment: string, readOnly: boolean, notes: string | null, 
/**
 * Engine-specific connection parameters (JSON object).
 */
pluginConfig: Record<string, unknown>, 
/**
 * True when a password is saved (vault, or an old keychain item).
 */
passwordSet: boolean, 
/**
 * False: the password is asked once per app session and kept in memory.
 */
savePassword: boolean, 
/**
 * SSH bastion (transport). Empty / disabled when unused.
 */
ssh: SshConfig, 
/**
 * True when a bastion password or key passphrase is saved.
 */
sshPasswordSet: boolean, 
/**
 * Always omitted on list/get. Write-only on save via [`SaveConnectionInput`].
 */
password?: string | null, favorite: boolean, 
/**
 * Query timeout for this profile. `None` uses the app setting; 0 is none.
 */
queryTimeoutSecs: number | null, lastConnectedAt: string | null, createdAt: string, updatedAt: string, };

export type ConstraintDraft = { kind: string, name: string, columns?: Array<string>, expression?: string | null, original?: ConstraintOriginal | null, drop?: boolean, };

/**
 * A CHECK or UNIQUE constraint that is not an index of its own.
 */
export type ConstraintOriginal = { 
/**
 * `check` or `unique`.
 */
kind: string, 
/**
 * Empty for an unnamed SQLite constraint.
 */
name: string, 
/**
 * UNIQUE columns.
 */
columns?: Array<string>, 
/**
 * CHECK expression, without the `CHECK` keyword.
 */
expression?: string | null, };

export type DdlPlan = { statements: Array<string>, warnings: Array<string>, transactional: boolean, requiresRebuild: boolean, 
/**
 * Shown with the preview. Not a warning, so it does not block apply.
 */
note: string | null, };

export type DescribeTableDefinitionRequest = { sessionId: string, table: string, schema?: string | null, database?: string | null, kind?: string | null, };

/**
 * Request to describe a single relation / collection.
 */
export type DescribeTableRequest = { sessionId: string, table: string, 
/**
 * SQL schema (Postgres) or MySQL database name when qualified that way.
 */
schema?: string | null, 
/**
 * Database / catalog name (MySQL schema, Mongo DB, optional elsewhere).
 */
database?: string | null, };

/**
 * Which parts of a schema diff to apply. Drops are opt-in: a dropped
 * table, column, index, foreign key, or constraint is applied only when
 * it is listed in `drops`.
 */
export type DiffSelection = { 
/**
 * Creates and alters to leave out, by change name.
 */
skip?: Array<string>, 
/**
 * Drops to apply.
 */
drops?: Array<DropKey>, };

/**
 * Distinct values for one catalog column. `limit` is clamped to 1000.
 */
export type DistinctColumnValuesRequest = { sessionId: string, table: string, schema?: string | null, database?: string | null, column: string, limit?: number, 
/**
 * Only values whose text starts with this (filter-value completion).
 */
prefix?: string | null, };

/**
 * A drop the diff found, and whether the selection includes it.
 */
export type DropItem = { kind: string, name: string, included: boolean, };

/**
 * One drop in a schema diff. `kind` is `table`, `column`, `index`,
 * `foreignKey`, or `constraint`; for `table`, `name` is the table.
 */
export type DropKey = { table: string, kind: string, name: string, };

/**
 * Capability flags returned to the UI / API consumers.
 */
export type EngineCapabilities = { engine: string, family: EngineFamily, queryLanguage: string, 
/**
 * sql-formatter dialect (`postgresql` / `mysql` / `sqlite` / `sql`). Empty for non-SQL.
 */
sqlDialect: string, supportsSchema: boolean, supportsForeignKeys: boolean, supportsErDiagram: boolean, supportsModelView: boolean, modelViewKind: ModelViewKind, supportsPagination: boolean, supportsWrite: boolean, supportsCancelQuery: boolean, 
/**
 * Manual commit on a pinned connection. Mongo is on only for a replica set.
 */
supportsTransactions: boolean, 
/**
 * Explain button. The UI does not parse engine text itself. Off unless
 * the plugin says so: the default `explain_query` is unsupported.
 */
supportsExplainPlan: boolean, 
/**
 * Structure tab can preview and apply a DDL plan. The UI does not build SQL.
 */
supportsStructureEdit: boolean, 
/**
 * Structure edits are limited to adding columns, renaming, and indexes.
 * No built-in engine sets it since the SQLite rebuild keeps the schema.
 */
structureEditLimited?: boolean, 
/**
 * Grid filter can ask for distinct column values. Hidden when false.
 */
supportsDistinctValues?: boolean, 
/**
 * Query editor gutter. Every runtime can run a statement.
 */
supportsQuery: boolean, supportsPooling: boolean, 
/**
 * Postgres / MySQL: switching database needs a new pool session.
 */
bindsSessionToDatabase: boolean, 
/**
 * Default database is the path segment of the connection URI (Mongo).
 */
preferDatabaseFromUri: boolean, 
/**
 * Home form + connection-detail facts layout.
 */
connectionKind: ConnectionKind, 
/**
 * Catalog names skipped when picking a default database.
 */
systemDatabases: Array<string>, 
/**
 * Example used as the connection-name placeholder.
 */
nameExample: string, objectLabels: ObjectLabels, explorer: ExplorerConfig, tableAdmin: TableAdminFeatures, 
/**
 * Placeholder for the grid code-filter box (plugin-owned).
 */
codeFilterPlaceholder: string, 
/**
 * File formats this plugin can export (`csv` / `json` / `jsonl` / `sql`).
 */
exportFormats: Array<string>, 
/**
 * Session-rail “Server metrics” tab (plugin `fetch_server_metrics`).
 */
supportsServerMetrics: boolean, 
/**
 * Terminate a backend session from the metrics tab.
 */
supportsTerminateSession: boolean, 
/**
 * Object explorer (catalog tree). Off at [`SupportDepth::Connect`].
 */
supportsExplorer: boolean, 
/**
 * Product honesty: `operate` | `browse` | `connect`.
 */
supportDepth: SupportDepth, };

export type EngineFamily = "RELATIONAL" | "DOCUMENT" | "WIDE_COLUMN" | "KEY_VALUE" | "OTHER";

/**
 * API / IPC request to run a statement against an open session.
 */
export type ExecuteQueryRequest = { sessionId: string, text: string, mode?: QueryMode, 
/**
 * Soft cap on returned rows (hard-capped in core).
 */
maxRows?: number | null, timeoutMs?: number | null, 
/**
 * Typed object name after a DROP/TRUNCATE confirm prompt.
 */
confirmObjectName?: string | null, 
/**
 * MongoDB: collection to query (required for document finds).
 */
collection?: string | null, 
/**
 * Optional database override (Mongo multi-DB explorer; falls back to session default).
 */
database?: string | null, 
/**
 * Client id for [`crate::cancel::QueryControl`]. Unknown ids cancel as a no-op.
 * Omitted ids are generated in core. Not stored on the connection profile.
 */
executionId?: string | null, 
/**
 * Bound values for `:name`, `?`, and `$n`. They are not spliced into `text`.
 */
params?: Array<JsonValue>, };

/**
 * Plugin-normalised plan plus the engine payload for the Raw tab.
 */
export type ExplainPlan = { 
/**
 * Server payload, unchanged. `raw_format` is `json` or `text`.
 */
raw: string, rawFormat: string, root: PlanNode, warnings?: Array<string>, };

/**
 * IPC body for `explain_engine_query`. Separate from [`crate::ExecuteQueryRequest`]
 * so execute call sites do not grow an analyze flag.
 */
export type ExplainQueryRequest = { sessionId: string, text: string, analyze?: boolean, mode?: QueryMode, timeoutMs?: number | null, confirmObjectName?: string | null, collection?: string | null, database?: string | null, executionId?: string | null, 
/**
 * Values for `:name` / `$1` / `?` placeholders, bound like a normal run.
 */
params?: Array<JsonValue>, };

/**
 * Explorer chrome inside one database (plugin-owned; FE must not hardcode).
 */
export type ExplorerConfig = { 
/**
 * `schema-tree` (Postgres) or `flat` (MySQL / SQLite / Mongo).
 */
layout: string, multiDatabase: boolean, canCreateDatabase: boolean, 
/**
 * Create a group/namespace (e.g. Postgres schema).
 */
canCreateGroup: boolean, 
/**
 * Create a leaf object (table / collection).
 */
canCreateObject: boolean, databaseLabel: string, groupLabel: string, objectLabel: string, objectLabelSingular: string, 
/**
 * Heading above the explorer list (e.g. "Tables", "Collections").
 */
listTitle: string, filterPlaceholder: string, };

export type ExportFormat = "csv" | "json" | "jsonl" | "sql";

/**
 * IPC request. `path` is the absolute destination file.
 */
export type ExportObjectRequest = { sessionId: string, name: string, schema?: string | null, database?: string | null, kind?: string | null, format: ExportFormat, path: string, 
/**
 * Cancel token for this export. The transfer queue owns it.
 */
executionId?: string | null, 
/**
 * A query tab whose open transaction the export reads through, so rows
 * it changed but has not committed are exported as the tab sees them.
 */
tabId?: string | null, };

export type ExportObjectResult = { path: string, rowCount: number, format: ExportFormat, };

export type FetchLargeValueRequest = { sessionId: string, locator: LargeCellLocator, offset?: number, limit?: number | null, executionId?: string | null, 
/**
 * A query tab with an open transaction reads the value through it, so
 * a row the tab changed but has not committed still loads as shown.
 */
tabId?: string | null, };

export type FetchLargeValueResult = { text: string, offset: number, nextOffset?: number | null, totalChars: number, };

/**
 * IPC request. `sessionId` is the Mystique pool session, not a backend pid.
 */
export type FetchServerMetricsRequest = { sessionId: string, includeSessions?: boolean, includeSlowQueries?: boolean, slowQueryLimit?: number | null, minDurationMs?: number | null, };

/**
 * IPC: fetch a page of table/collection rows (plugin-owned SQL / find + order).
 */
export type FetchTablePreviewRequest = { sessionId: string, table: string, schema?: string | null, database?: string | null, maxRows?: number | null, offset?: number | null, 
/**
 * Plugin-compiled predicates (AND). Empty = unfiltered.
 */
filters?: Array<TableFilter>, 
/**
 * Raw code-filter box. Plugins compile JSON `{col: val}` or `col = val` expressions.
 */
codeFilter?: string | null, 
/**
 * Column to `ORDER BY` / Mongo `sort`. Must match a described column.
 * Empty / unknown = plugin default (PK / `_id` / `rowid`).
 */
sortColumn?: string | null, 
/**
 * `asc` | `desc`. Ignored when `sort_column` is empty.
 */
sortDir?: string | null, 
/**
 * Same id the query canvas uses, so closing the tab cancels the page load.
 */
executionId?: string | null, };

/**
 * Deferred COUNT + FK targets. Does not block first paint of a table.
 */
export type FetchTablePreviewStatsRequest = { sessionId: string, table: string, schema?: string | null, database?: string | null, filters?: Array<TableFilter>, codeFilter?: string | null, 
/**
 * Lets the grid cancel the count with `cancel_engine_query`.
 */
executionId?: string | null, 
/**
 * Stop the exact COUNT after this long and report the engine's estimate
 * instead. Absent: count to the end (the user asked for an exact count).
 * Under a budget, an unfiltered table estimated at
 * [`SKIP_EXACT_COUNT_ROWS`] or more is not counted at all.
 */
countBudgetMs?: number | null, };

export type ForeignKeyDraft = { name: string, columns: Array<string>, refSchema?: string | null, refTable: string, refColumns: Array<string>, original?: ForeignKeyOriginal | null, drop?: boolean, onDelete?: string | null, onUpdate?: string | null, };

export type ForeignKeyOriginal = { name: string, columns: Array<string>, refSchema?: string | null, refTable: string, refColumns: Array<string>, 
/**
 * `CASCADE`, `SET NULL`, `SET DEFAULT`, `RESTRICT`. `None` is NO ACTION.
 */
onDelete?: string | null, onUpdate?: string | null, };

/**
 * `format_engine_connection_url` request.
 */
export type FormatConnectionUrlRequest = { engine: string, pluginConfig: Record<string, unknown>, password?: string | null, };

/**
 * Result of formatting discrete plugin config into a connection URL.
 *
 * Inverse of [`ParsedConnectionUrl`]: plugins own scheme rules so the FE
 * never reimplements `postgresql://` / `mysql://` / `mongodb://` assembly.
 */
export type FormattedConnectionUrl = { engine: string, url: string, };

/**
 * Frozen, hidden, ordered, and resized columns for one table on one connection.
 * `table_id` is `database.schema.name`, so the key covers all four parts.
 */
export type GridColumnLayoutRecord = { connectionId: string, tableId: string, order?: Array<string>, hidden?: Array<string>, frozen?: Array<string>, 
/**
 * Widths the user set, by column name, in unscaled CSS pixels.
 */
widths?: { [key in string]: number }, };

/**
 * What the grid needs to offer rollback. No SQL.
 */
export type GridSaveSummary = { saveId: string, createdAt: string, };

export type ImportColumnMap = { fileKey: string, tableColumn?: string | null, };

export type ImportEncoding = "utf8" | "utf16Le" | "utf16Be" | "windows1252" | "iso88591";

export type ImportFormat = "csv" | "json" | "jsonl" | "sql";

export type ImportObjectRequest = { sessionId: string, name: string, schema?: string | null, database?: string | null, kind?: string | null, path: string, format: ImportFormat, encoding?: ImportEncoding, mapping?: Array<ImportColumnMap>, onDuplicate?: ImportOnDuplicate, 
/**
 * Cancel token for this import. The transfer queue owns it.
 */
executionId?: string | null, };

export type ImportObjectResult = { inserted: number, updated: number, format: ImportFormat, message?: string | null, };

export type ImportOnDuplicate = "stopOnError" | "updateOnDuplicate";

export type ImportTableColumn = { name: string, dataType: string, pk: boolean, nullable: boolean, };

export type IndexDraft = { name: string, columns: Array<string>, unique: boolean, primary?: boolean, 
/**
 * Postgres `CREATE INDEX CONCURRENTLY`. Cannot share a transaction.
 */
concurrent?: boolean, original?: IndexOriginal | null, drop?: boolean, 
/**
 * See [`IndexOriginal::definition`]. Used to create the index when its
 * columns were not edited.
 */
definition?: string | null, };

export type IndexOriginal = { name: string, columns: Array<string>, unique: boolean, primary?: boolean, 
/**
 * The full `CREATE INDEX` as the engine prints it (`pg_get_indexdef`,
 * `sqlite_master.sql`), so partial, expression, and INCLUDE indexes
 * are replayed as they are.
 */
definition?: string | null, };

/**
 * Stable machine kind of an IPC failure. snake_case on the wire.
 */
export type IpcErrorKind = "invalid_config" | "auth" | "connect" | "pool" | "timeout" | "not_supported" | "query" | "policy" | "write_guard" | "ssh_host_key" | "password_required" | "rollback_stale" | "not_found" | "invalid_input" | "storage" | "secret_store" | "internal";

export type IpcErrorPayload = { kind: IpcErrorKind, message: string, 
/**
 * Where a query fault sits in the submitted text.
 */
position?: SourcePosition | null, 
/**
 * 0-based statement of a failed batch.
 */
statementIndex?: number | null, prompt?: IpcPrompt | null, 
/**
 * A manual-commit tab's transaction state after the failed run.
 */
transactionOutcome?: PinStatus | null, };

/**
 * What the UI asks before it retries.
 */
export type IpcPrompt = { "type": "writeGuard" } & WriteGuardPrompt | { "type": "sshHostKey" } & SshHostKeyPrompt | { "type": "passwordRequired" };

export type JsonValue = unknown;

/**
 * One key value, already in the js-safe JSON form the grid uses.
 * `data_type` lets the engine bind it as that type (a MySQL integer key
 * is bound as an integer, a decimal one cast on the parameter side), so
 * the column is never cast and the key index is used.
 */
export type LargeCellKey = { column: string, value: JsonValue, dataType?: string | null, };

/**
 * Handed back by the grid to load the rest of one cell. No SQL text.
 */
export type LargeCellLocator = { database?: string | null, schema?: string | null, table: string, column: string, keys: Array<LargeCellKey>, };

/**
 * Optional scope for bulk column listing.
 */
export type ListCatalogColumnsRequest = { sessionId: string, 
/**
 * Limit to one database (MySQL / Mongo).
 */
database?: string | null, 
/**
 * Limit to one schema (Postgres).
 */
schema?: string | null, };

export type ListCatalogRequest = { sessionId: string, preferredDatabase?: string | null, 
/**
 * When false, plugins return database names only (no tables/collections).
 */
includeObjects?: boolean, };

export type ListRelationshipsRequest = { sessionId: string, database?: string | null, };

export type ListTableDefinitionsRequest = { sessionId: string, database?: string | null, schema?: string | null, };

export type McpHarness = { id: string, label: string, mark: string, hint: string, configPath: string, installed: boolean, stale: boolean, };

export type McpStatus = { enabled: boolean, running: boolean, port: number | null, url: string | null, token: string | null, };

/**
 * How the UI should render a metric.
 */
export type MetricKind = "count" | "bytes" | "durationMs" | "percent" | "text" | "ratio";

/**
 * One typed metric. `value` is raw; `display` is an optional plugin hint.
 */
export type MetricValue = { key: string, label: string, kind: MetricKind, value: number | null, display: string | null, unit: string | null, 
/**
 * Gauge max (e.g. max connections).
 */
max: number | null, available: boolean, unavailableReason: string | null, };

export type ModelViewKind = "ER" | "COLLECTION_MAP" | "KEY_MODEL" | "NONE";

export type ObjectLabels = { namespace: string, container: string, field: string, };

/**
 * Full structure payload returned by a plugin.
 */
export type ObjectStructure = { title: string, subtitle?: string | null, objectKind?: string | null, sections: Array<StructureSection>, };

/**
 * Request structure for one table / view / collection.
 */
export type ObjectStructureRequest = { sessionId: string, table: string, schema?: string | null, database?: string | null, 
/**
 * `table` | `view` | `collection` (hint only; plugins may ignore).
 */
kind?: string | null, };

/**
 * API request to open a pooled session.
 */
export type OpenSessionRequest = { engine: string, pluginConfig: Record<string, unknown>, password?: string | null, pool?: PoolConfig | null, 
/**
 * Optional client-supplied id (defaults to UUID).
 */
sessionId?: string | null, 
/**
 * When `password` is empty, resolve the OS-keychain secret for this profile.
 */
profileId?: string | null, 
/**
 * Optional one-hop SSH tunnel. Plugins still see 127.0.0.1.
 */
ssh?: SshTunnelRequest | null, 
/**
 * Passphrase for an encrypted TLS client key. Never persisted in plugin_config.
 */
tlsKeyPassphrase?: string | null, 
/**
 * Profile environment (`local` | `staging` | `production`). Filled from the profile.
 */
environment?: string | null, 
/**
 * Connection display name for write-guard prompts.
 */
connectionName?: string | null, 
/**
 * When false, an omitted `ssh` stays omitted. The test form sets this
 * from the SSH checkbox so an unticked box does not reuse the saved tunnel.
 */
useSavedSsh?: boolean, };

/**
 * `parse_engine_connection_url` request.
 */
export type ParseConnectionUrlRequest = { engine: string, url: string, };

/**
 * Result of parsing a connection URL into discrete plugin config fields.
 *
 * Plugins map engine-native URL forms (`postgres://…`, `mysql://…`) onto the
 * same `plugin_config` keys used by the connection form and `build_*` helpers.
 */
export type ParsedConnectionUrl = { engine: string, 
/**
 * Discrete fields (host, port, database, username, ssl, …).
 */
pluginConfig: Record<string, unknown>, 
/**
 * Password from the URL, if present (never log this).
 */
password?: string | null, };

/**
 * What the editor shows for one tab.
 */
export type PinStatus = { pinned: boolean, inTransaction: boolean, 
/**
 * An open transaction was rolled back: on unpin, or without the user
 * asking (idle timeout, disconnect). Nothing in it was committed.
 */
rolledBack: boolean, };

export type PlanDdlRequest = { sessionId: string, draft: TableDefinition, };

/**
 * One labelled fact an engine actually returned (`index`, `actual time`).
 */
export type PlanField = { label: string, value: string, };

/**
 * One node in an explain tree.
 *
 * `cost` and `rows` are omitted when the engine has no such number.
 * Self cost is `cost − Σ children` and is computed by the UI.
 */
export type PlanNode = { name: string, rows?: number | null, cost?: number | null, fields?: Array<PlanField>, children?: Array<PlanNode>, };

export type PlanSchemaDiffRequest = { sessionId: string, source: Array<TableDefinition>, target: Array<TableDefinition>, renames?: Array<RenamePair>, 
/**
 * Changes to skip and drops to include. Drops are off unless listed.
 */
selection?: DiffSelection, };

/**
 * Cross-engine pool knobs. Plugins map these onto native pool settings.
 */
export type PoolConfig = { 
/**
 * Max connections in the pool (default engine-specific).
 */
maxSize?: number, 
/**
 * Minimum idle connections to keep warm.
 */
minIdle?: number, 
/**
 * Acquire timeout in milliseconds.
 */
acquireTimeoutMs?: number, 
/**
 * Connection idle timeout in seconds (0 = no timeout).
 */
idleTimeoutSecs?: number, };

/**
 * Snapshot of pool health for API / status bar.
 */
export type PoolStatus = { engine: string, maxSize: number, 
/**
 * Currently checked-out connections (approx).
 */
active: number, 
/**
 * Idle connections available in the pool.
 */
idle: number, 
/**
 * Total connections managed by the pool.
 */
size: number, };

export type PreviewImportRequest = { sessionId: string, name: string, schema?: string | null, database?: string | null, kind?: string | null, path: string, format: ImportFormat, encoding?: ImportEncoding, };

export type PreviewImportResult = { fileKeys: Array<string>, columns: Array<ImportTableColumn>, suggestedMap: Array<ImportColumnMap>, rows: Array<Array<JsonValue>>, format: ImportFormat, encoding: ImportEncoding, };

export type PreviewWriteRowsRequest = { sessionId: string, 
/**
 * One statement from the write dialog.
 */
statement: string, database?: string | null, };

export type PurgeReport = { 
/**
 * False when this build does not use the keychain (file key source).
 */
supported: boolean, deleted: number, 
/**
 * Items that would have needed a prompt to delete.
 */
skipped: number, };

/**
 * Every statement that ran, plus the 0-based index that stopped the batch.
 *
 * A mid-batch statement error is `Ok` with `stopped_at` set so earlier result
 * sets stay visible. Read-mode rejection is a [`ConnectorError`] instead:
 * nothing ran, so `results` would be empty.
 */
export type QueryBatchResult = { results: Array<StatementResult>, stoppedAt: number | null, };

/**
 * One result column.
 */
export type QueryColumn = { name: string, 
/**
 * Engine type label (`int8`, `numeric`, `text`).
 *
 * Integers outside ±(2^53−1) and every numeric/decimal cell are JSON
 * strings. This tag is how the grid tells that string from ordinary text
 * and can right-align it.
 */
dataType: string | null, };

export type QueryHistoryItem = { id: string, connectionId: string | null, engine: string | null, body: string, language: string | null, success: boolean, rowCount: number | null, durationMs: number | null, errorMessage: string | null, ranAt: string, 
/**
 * Catalog or database name. Never a connection URL.
 */
database: string | null, affectedRows: number | null, source: QueryHistorySource, kind: QueryHistoryKind, };

/**
 * What wrote the row. A query run is `query`; the rest are app actions the
 * history lists so a change can be traced.
 */
export type QueryHistoryKind = "query" | "grid_save" | "ddl" | "rollback" | "explain_analyze";

export type QueryHistoryPage = { items: Array<QueryHistoryItem>, hasMore: boolean, };

/**
 * Who ran the statement. The connection manager sets this from the session.
 * The query canvas does not.
 */
export type QueryHistorySource = "ui" | "agent" | "mcp";

/**
 * Read vs write. Connection profile `read_only` forces Read.
 */
export type QueryMode = "read" | "write";

/**
 * Tabular query result for the FE grid.
 */
export type QueryResult = { engine: string, mode: QueryMode, columns: Array<QueryColumn>, 
/**
 * Row-major cells. Large integers and numerics are strings; see [`QueryColumn::data_type`].
 */
rows: Array<Array<JsonValue>>, rowCount: number, truncated: boolean, durationMs: number, 
/**
 * Optional human note (e.g. "validated write — not executed").
 */
message: string | null, };

export type RenamePair = { 
/**
 * Name on the source side (the name to end with).
 */
from: string, 
/**
 * Name on the target side (the name it has now).
 */
to: string, 
/**
 * Set for a column pair: the source-side table the columns belong to.
 * `None` pairs two tables.
 */
table?: string | null, 
/**
 * The user said these two columns are not a rename. Stops the guess.
 */
keepApart?: boolean, };

/**
 * Replace all open tabs for a connection (order = array order).
 */
export type ReplaceWorkspaceTabsInput = { connectionId: string, tabs: Array<SaveWorkspaceTabInput>, 
/**
 * Which tab id is active (optional; also set via is_active on items).
 */
activeTabId?: string | null, };

/**
 * Roll back one grid save. The backend owns the inverse SQL; the client
 * sends only the id it got back from the save.
 */
export type RollbackTableEditsRequest = { sessionId: string, saveId: string, confirmObjectName?: string | null, };

export type SaveConnectionInput = { id?: string | null, name: string, engine: string, colorId: string, environment: string, readOnly: boolean, notes?: string | null, pluginConfig: Record<string, unknown>, 
/**
 * Plaintext password to store. Empty / omitted keeps the existing secret on update.
 */
password?: string | null, 
/**
 * Legacy alias: if `password` is unset, non-empty `secret_ref` is treated as the password
 * when it is not a keychain pointer or placeholder.
 */
secretRef?: string | null, 
/**
 * When true, clear the stored password on update.
 */
clearSecret?: boolean, 
/**
 * After insert, copy the keychain secret from this profile id (duplicate).
 */
copySecretFrom?: string | null, 
/**
 * `None` leaves the stored flag unchanged on update.
 */
favorite?: boolean | null, 
/**
 * SSH bastion. Omitted / default = no tunnel.
 */
sshConfig?: SshConfig, 
/**
 * Bastion password or key passphrase. Empty keeps the existing secret.
 */
sshPassword?: string | null, 
/**
 * Clear the stored SSH secret on update.
 */
clearSshSecret?: boolean, 
/**
 * Passphrase for an encrypted TLS client key.
 */
tlsKeyPassphrase?: string | null, 
/**
 * `None` uses the app setting; 0 is no timeout. Saved as sent, also on update.
 */
queryTimeoutSecs?: number | null, 
/**
 * False deletes any saved DB password and asks once per session.
 * `None` keeps the stored flag on update and saves on create.
 */
savePassword?: boolean | null, };

export type SaveQueryInput = { id?: string | null, connectionId: string, name: string, body: string, language?: string | null, sortOrder?: number, 
/**
 * `None` leaves the stored flag unchanged on update.
 */
favorite?: boolean | null, };

export type SaveWorkspaceTabInput = { id: string, connectionId: string, kind: string, title: string, sortOrder?: number, isActive?: boolean, savedQueryId?: string | null, body?: string | null, language?: string | null, writeEnabled?: boolean, tableId?: string | null, modelDatabase?: string | null, isDirty?: boolean, };

/**
 * Named query saved for a connection (no groups).
 */
export type SavedQuery = { id: string, connectionId: string, name: string, body: string, language: string | null, sortOrder: number, favorite: boolean, createdAt: string, updatedAt: string, };

export type SchemaChange = { name: string, action: string, plan: DdlPlan, draft: TableDefinition, 
/**
 * Applied with the migration: false for a skipped change, or a table
 * drop that was not chosen.
 */
selected: boolean, 
/**
 * Drops inside this change (for `drop`, the table itself).
 */
drops: Array<DropItem>, 
/**
 * Column renames in this change.
 */
columnRenames: Array<ColumnRename>, };

export type SchemaMigration = { changes: Array<SchemaChange>, warnings: Array<string>, };

export type SendAgentMessage = { connectionId: string, connectionName: string, engine: string, database: string | null, chatId: string | null, provider: string, model: string, text: string, 
/**
 * Live editor buffer for the active query tab. Not persisted as a chat message.
 */
editorQuery: string | null, 
/**
 * `@table` references: their columns are looked up and sent with the turn.
 */
mentions?: Array<AgentTableRef>, 
/**
 * Set by "Fix with AI": the turn opens with the repair prompt.
 */
fix?: AgentFixRequest, };

/**
 * The run started: the chat (new or existing) and the saved user message.
 * Everything else arrives on the event channel, ending in `done` or `error`.
 */
export type SendAgentResult = { chat: AgentChat, user: AgentMessage, };

/**
 * What this engine can populate in the metrics tab.
 */
export type ServerMetricsCapabilities = { overview: boolean, activeSessions: boolean, terminateSession: boolean, slowQueries: boolean, 
/**
 * Human notes (missing extensions, file-engine limits, …).
 */
notes: Array<string>, };

/**
 * Full metrics payload returned to the UI.
 */
export type ServerMetricsSnapshot = { engine: string, collectedAt: string, capabilities: ServerMetricsCapabilities, overview: ServerOverview, sessions: Array<ActiveBackendSession>, slowQueries: Array<SlowQuery>, };

/**
 * Headline server snapshot. Keys are stable so the FE can layout them.
 */
export type ServerOverview = { connections: MetricValue, cacheHitRatio: MetricValue, databaseSize: MetricValue, uptime: MetricValue, activeQueries: MetricValue, 
/**
 * Engine-specific extras (WAL mode, WiredTiger cache, Questions, …).
 */
extras: Array<MetricValue>, };

/**
 * Live session handle returned to API clients.
 */
export type SessionInfo = { sessionId: string, engine: string, displayName: string, capabilities: EngineCapabilities, pool: PoolStatus | null, openedAt: string, };

/**
 * A currently-running or historically slow statement.
 */
export type SlowQuery = { id: string, query: string, durationMs: number | null, meanMs: number | null, calls: number | null, database: string | null, user: string | null, 
/**
 * `live` (running now) or `historical` (pg_stat_statements / digest).
 */
source: string | null, extras: Array<MetricValue>, };

/**
 * Where a fault sits in the submitted batch.
 */
export type SourcePosition = { 
/**
 * 0-based byte offset.
 */
offset: number, 
/**
 * 1-based line.
 */
line: number, 
/**
 * 1-based column, counting characters.
 */
column: number, };

/**
 * Byte range of one statement in the submitted batch.
 */
export type SqlRange = { start: number, end: number, };

/**
 * How we authenticate to the bastion. One hop; no SOCKS/HTTP.
 */
export type SshAuth = "password" | "key" | "agent";

/**
 * Persisted SSH fields. Secrets never live here — see `ssh_secret_ref`.
 */
export type SshConfig = { enabled?: boolean, host?: string, port?: number, user?: string, auth?: SshAuth, privateKeyPath?: string, };

/**
 * TOFU / mismatch prompt for an SSH bastion host key.
 */
export type SshHostKeyPrompt = { 
/**
 * Always `sshHostKey`. Kept on the wire for older readers.
 */
code: string, 
/**
 * `unknown` (first connect) or `mismatch` (key changed).
 */
kind: string, host: string, port: number, fingerprint: string, keyType: string, publicKey?: string, };

/**
 * Runtime tunnel params on `OpenSessionRequest` (includes secrets).
 */
export type SshTunnelRequest = { host: string, port: number, user: string, auth: SshAuth, password?: string | null, privateKeyPath?: string | null, passphrase?: string | null, 
/**
 * SHA256 fingerprint already trusted (known_hosts or this-session accept).
 */
trustedFingerprint?: string | null, };

/**
 * Risk copied from the SQL planner. Session is not a data write.
 */
export type StatementKind = "read" | "session" | "write" | "destructive";

/**
 * One statement outcome. `index` is 0-based. Extra MySQL `CALL` sets share
 * `index` and increment `set_index`.
 */
export type StatementResult = { index: number, setIndex: number, sqlRange: SqlRange, kind: StatementKind, result: QueryResult | null, affectedRows: number | null, error: string | null, 
/**
 * Byte offset in the submitted batch. Absent when the engine named no character.
 */
errorPosition?: SourcePosition | null, notices: Array<string>, };

/**
 * One named section of structure (columns, indexes, …).
 */
export type StructureSection = { id: string, title: string, description?: string | null, 
/**
 * Column headers for the table body.
 */
columns: Array<string>, 
/**
 * Rows of display strings (aligned to [`columns`]).
 */
rows: Array<Array<string>>, emptyMessage?: string | null, };

/**
 * How much of the IDE this product actually supports.
 *
 * Chrome reads the boolean flags this depth clamps — never `if engine ===`.
 */
export type SupportDepth = "operate" | "browse" | "connect";

/**
 * Catalog-object actions the plugin implements (`apply_table_admin`).
 */
export type TableAdminFeatures = { createStatement: boolean, rename: boolean, drop: boolean, truncate: boolean, duplicate: boolean, 
/**
 * Right-click “View structure” (plugin `describe_object_structure`).
 */
viewStructure: boolean, };

export type TableAdminOp = "createStatement" | "createGroup" | "createObject" | "rename" | "drop" | "truncate" | "duplicate";

/**
 * IPC request.
 */
export type TableAdminRequest = { sessionId: string, op: TableAdminOp, name: string, schema?: string | null, database?: string | null, kind?: string | null, 
/**
 * Required for rename + duplicate.
 */
newName?: string | null, confirmObjectName?: string | null, };

export type TableAdminResult = { op: TableAdminOp, 
/**
 * Populated for [`TableAdminOp::CreateStatement`].
 */
statement?: string | null, executed: boolean, message?: string | null, };

/**
 * How the exact COUNT of a [`TablePreviewCount`] ended.
 */
export type TableCountOutcome = "exact" | "over_budget" | "skipped" | "cancelled" | "failed";

export type TableDefinition = { database?: string | null, schema?: string | null, name: string, kind?: string, columns?: Array<ColumnDraft>, indexes?: Array<IndexDraft>, foreignKeys?: Array<ForeignKeyDraft>, 
/**
 * CHECK and UNIQUE constraints (UNIQUE ones that are only an index
 * on this engine stay in `indexes`).
 */
constraints?: Array<ConstraintDraft>, 
/**
 * Triggers on the table (SQLite: replayed after a rebuild).
 */
triggers?: Array<TriggerDefinition>, 
/**
 * SQLite `sqlite_master.sql` captured when the definition was read.
 */
schemaRevision?: string | null, validator?: string | null, originalValidator?: string | null, create?: boolean, dropTable?: boolean, 
/**
 * After an explicit rename pair, the table should end with this name.
 */
renameTo?: string | null, };

/**
 * Column metadata needed to compile a write (types, PK).
 */
export type TableEditColumn = { name: string, dataType?: string, nullable?: boolean, pk?: boolean, };

/**
 * One grid filter clause. Plugins compile this to SQL / MQL — never the FE.
 */
export type TableFilter = { column: string, 
/**
 * `eq` `neq` `gt` `gte` `lt` `lte` `contains` `starts` `ends`
 * `is_null` `not_null` `is_empty` `not_empty`
 */
op: string, value?: string, };

export type TablePreviewColumnRef = { column: string, references: ColumnReference, };

/**
 * The grid's background count: [`TablePreviewStats`] plus what happened to
 * the exact COUNT, and the engine's estimate when the count did not finish.
 */
export type TablePreviewCount = { totalRows: number, totalKnown: boolean, references: Array<TablePreviewColumnRef>, outcome: TableCountOutcome, 
/**
 * Planner estimate (Postgres / MySQL `EXPLAIN`) when not exact.
 */
estimatedRows?: number, };

/**
 * Stable preview payload: columns + rows in plugin-defined order.
 */
export type TablePreviewResult = { engine: string, columns: Array<ColumnInfo>, rows: Array<Array<JsonValue>>, rowCount: number, 
/**
 * Full table size (for pager). Meaningful only when `total_known`.
 */
totalRows: number, 
/**
 * False on the fast path (COUNT runs later). Pager must not treat
 * `total_rows` as authoritative until this is true.
 */
totalKnown: boolean, truncated: boolean, durationMs: number, };

export type TablePreviewStats = { totalRows: number, totalKnown: boolean, references: Array<TablePreviewColumnRef>, };

/**
 * Delete one existing row. `original` locates the row (PKs).
 */
export type TableRowDelete = { original: { [key in string]: JsonValue }, };

/**
 * Insert one new row. `values` omits empty cells so defaults can apply.
 */
export type TableRowInsert = { values: { [key in string]: JsonValue }, };

/**
 * Update one existing row. `original` is used to locate the row (PKs);
 * `values` is the changed column → new value map.
 */
export type TableRowUpdate = { original: { [key in string]: JsonValue }, values: { [key in string]: JsonValue }, };

/**
 * IPC request to kill a backend session.
 */
export type TerminateSessionRequest = { sessionId: string, 
/**
 * Engine-native id from [`ActiveBackendSession::id`].
 */
backendSessionId: string, };

export type TerminateSessionResult = { ok: boolean, backendSessionId: string, message: string | null, };

export type TestResult = { ok: boolean, engine: string, 
/**
 * Server / driver version if known.
 */
serverVersion: string | null, latencyMs: number, message: string, sshHostKey?: SshHostKeyPrompt | null, };

/**
 * Whether writes are allowed on a read-only session, and for how long.
 */
export type TimedUnlockStatus = { sessionId: string, 
/**
 * Milliseconds left; `None` when no unlock is running.
 */
remainingMs?: number | null, };

export type ToolAccessConnection = { id: string, name: string, engine: string, 
/**
 * `local` | `staging` | `production`.
 */
environment: string, 
/**
 * The profile's default database, when it names one.
 */
defaultDatabase?: string | null, 
/**
 * Set on the connection. Absent = inherits the global default.
 */
level?: string | null, 
/**
 * What MCP gets on the connection (after inheritance).
 */
mcpLevel: string, 
/**
 * What the built-in agent gets on the connection (after inheritance).
 */
agentLevel: string, 
/**
 * Writes above the level go to the approval dock instead of failing.
 */
askHighRisk: boolean, 
/**
 * Levels set on single databases.
 */
databases: Array<ToolAccessDatabase>, };

export type ToolAccessDatabase = { name: string, level: string, };

/**
 * The settings pane: both global defaults and every connection's levels.
 */
export type ToolAccessView = { 
/**
 * `none` | `read` | `write` | `full`.
 */
mcpDefault: string, agentDefault: string, connections: Array<ToolAccessConnection>, };

/**
 * One write waiting in the approval dock. Never persisted.
 */
export type ToolApprovalRequest = { id: string, 
/**
 * `mcp` | `agent`.
 */
caller: string, connectionId: string, connectionName: string, database?: string | null, environment: string, 
/**
 * Exactly the text that runs if approved.
 */
sql: string, 
/**
 * The level the statement needs, and the level the caller has.
 */
needed: string, granted: string, requestedAt: string, 
/**
 * After this the request is denied.
 */
expiresAt: string, };

/**
 * Rows exported or imported so far, sent over IPC while a transfer runs.
 */
export type TransferProgress = { rows: number, };

/**
 * A trigger on the table, as the engine stores it. Read only: SQLite
 * replays it after a table rebuild.
 */
export type TriggerDefinition = { name: string, sql: string, };

/**
 * `update_agent_chat` request. `None` leaves a field as it is.
 */
export type UpdateAgentChatInput = { id: string, title?: string | null, provider?: string | null, model?: string | null, };

export type VaultStatus = { 
/**
 * `keychain`, `file` or `memory`.
 */
source: string, 
/**
 * `idle` (not loaded yet), `unlocked` or `denied`.
 */
state: string, message: string | null, };

/**
 * Persisted workspace tab (query canvas, table grid, or ER model).
 */
export type WorkspaceTabRecord = { id: string, connectionId: string, 
/**
 * `query` | `table` | `model`
 */
kind: string, title: string, sortOrder: number, isActive: boolean, 
/**
 * Linked saved query when this is a saved query tab.
 */
savedQueryId: string | null, 
/**
 * Draft SQL/MQL body for query tabs.
 */
body: string | null, language: string | null, writeEnabled: boolean, 
/**
 * Table id for kind === table.
 */
tableId: string | null, 
/**
 * Database key for kind === model.
 */
modelDatabase: string | null, 
/**
 * Unsaved edits (query draft dirty, or table grid dirty).
 */
isDirty: boolean, createdAt: string, updatedAt: string, };

export type WriteGuardPrompt = { code: string, 
/**
 * `unlock` (allow writes this session), `confirm` (one dialog, no unlock)
 * or `typeConfirm` (type the object name).
 */
kind: string, environment: string, connectionName: string, 
/**
 * Short description of the write.
 */
preview: string, 
/**
 * Statements the write will run, when known.
 */
statements?: Array<string>, 
/**
 * Name to send back as `confirmObjectName` for `confirm` and `typeConfirm`.
 */
objectName?: string | null, 
/**
 * Production only: things to look at before confirming, such as an
 * `UPDATE` / `DELETE` with no `WHERE` or one that is always true.
 * A warning, never a block.
 */
warnings?: Array<string>, };

/**
 * The rows one write would touch, or why there is no preview.
 */
export type WriteRowPreview = { 
/**
 * False: `reason` says why there is no preview.
 */
available: boolean, reason?: string | null, 
/**
 * `update` | `delete` | `insert`.
 */
action?: string | null, table?: string | null, columns: Array<QueryColumn>, rows: Array<Array<JsonValue>>, 
/**
 * UPDATE: the `"<column> (new)"` columns at the end of each row.
 */
newColumns: Array<string>, 
/**
 * How many rows the write touches. `None`: the count did not finish.
 */
total?: number | null, 
/**
 * More rows than `rows` holds.
 */
truncated: boolean, 
/**
 * The read failed (timeout, permissions). The write is not allowed by it.
 */
error?: string | null, 
/**
 * Why `total` is missing.
 */
note?: string | null, };
