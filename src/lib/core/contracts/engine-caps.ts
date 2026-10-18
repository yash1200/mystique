/**
 * Plugin-owned engine config (mirrors EngineCapabilities from connector-api).
 * The FE must render chrome from this — not from `if engine === "mongodb"`.
 */

/** High-level data family for live ER / model views. */
export type DataFamily = 'sql' | 'nosql' | 'redis'

export type EngineFamily =
	| 'RELATIONAL'
	| 'DOCUMENT'
	| 'WIDE_COLUMN'
	| 'KEY_VALUE'
	| 'OTHER'

export type ModelViewKind = 'ER' | 'COLLECTION_MAP' | 'KEY_MODEL' | 'NONE'

export type ExplorerLayout = 'schema-tree' | 'flat'

export type ConnectionKind = 'host' | 'uri' | 'file'

/** How much of the IDE this product actually supports. */
export type SupportDepth = 'operate' | 'browse' | 'connect'

export interface ObjectLabels {
	namespace: string
	container: string
	field: string
}

export interface ExplorerConfig {
	layout: ExplorerLayout | string
	multiDatabase: boolean
	canCreateDatabase: boolean
	/** Create a group/namespace (e.g. Postgres schema). */
	canCreateGroup: boolean
	/** Create a leaf object (table / collection). */
	canCreateObject: boolean
	databaseLabel: string
	groupLabel: string
	objectLabel: string
	objectLabelSingular: string
	/** Heading above the explorer list (e.g. Tables / Collections). */
	listTitle: string
	filterPlaceholder: string
}

export interface TableAdminFeatures {
	createStatement: boolean
	rename: boolean
	drop: boolean
	truncate: boolean
	duplicate: boolean
	viewStructure: boolean
}

export interface EngineCapabilities {
	engine: string
	family: EngineFamily
	queryLanguage: string
	/** sql-formatter dialect (`postgresql` / `mysql` / `sqlite` / `sql`). Empty for non-SQL. */
	sqlDialect: string
	supportsSchema: boolean
	supportsForeignKeys: boolean
	supportsErDiagram: boolean
	supportsModelView: boolean
	modelViewKind: ModelViewKind
	supportsPagination: boolean
	supportsWrite: boolean
	supportsCancelQuery: boolean
	/** Manual commit on a pinned connection. Off until a Mongo replica set is detected. */
	supportsTransactions?: boolean
	/** Explain button. Missing means the engine did not advertise it. */
	supportsExplainPlan?: boolean
	/** Structure tab can add, alter, and drop columns, indexes, and foreign keys. */
	supportsStructureEdit?: boolean
	/** Grid filter can list distinct values. Missing means the engine did not opt out. */
	supportsDistinctValues?: boolean
	/** Query gutter. One implementation for every engine that can run a statement. */
	supportsQuery: boolean
	supportsPooling: boolean
	bindsSessionToDatabase: boolean
	/** Default database is the path segment of the connection URI (Mongo). */
	preferDatabaseFromUri: boolean
	/** Home form + connection-detail facts layout. */
	connectionKind: ConnectionKind
	/** Catalog names skipped when picking a default database. */
	systemDatabases: string[]
	/** Example used as the connection-name placeholder. */
	nameExample: string
	objectLabels: ObjectLabels
	explorer: ExplorerConfig
	tableAdmin: TableAdminFeatures
	/** Grid code-filter placeholder from the plugin. */
	codeFilterPlaceholder: string
	/** Plugin-owned file export formats (`csv` / `json` / `jsonl` / `sql`). */
	exportFormats: string[]
	/** Session-rail “Server metrics” tab. */
	supportsServerMetrics: boolean
	/** Terminate a backend session from the metrics tab. */
	supportsTerminateSession: boolean
	/** Object explorer. Off at `connect` depth. */
	supportsExplorer: boolean
	/** Product honesty: operate | browse | connect. */
	supportDepth: SupportDepth
}
