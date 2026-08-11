/**
 * Plugin-owned engine config (mirrors EngineCapabilities from connector-api).
 * The FE must render chrome from this — not from `if engine === "mongodb"`.
 */
import type { DataFamily } from './er'

export type EngineFamily =
	| 'RELATIONAL'
	| 'DOCUMENT'
	| 'WIDE_COLUMN'
	| 'KEY_VALUE'
	| 'OTHER'

export type ModelViewKind = 'ER' | 'COLLECTION_MAP' | 'KEY_MODEL' | 'NONE'

export type ExplorerLayout = 'schema-tree' | 'flat'

export type ConnectionKind = 'host' | 'uri' | 'file'

export interface ObjectLabels {
	namespace: string
	container: string
	field: string
}

export interface ExplorerConfig {
	layout: ExplorerLayout | string
	multiDatabase: boolean
	canCreateDatabase: boolean
	databaseLabel: string
	groupLabel: string
	objectLabel: string
	objectLabelSingular: string
	filterPlaceholder: string
}

export interface TableAdminFeatures {
	createStatement: boolean
	rename: boolean
	drop: boolean
	truncate: boolean
	duplicate: boolean
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
}

const SQL_ADMIN: TableAdminFeatures = {
	createStatement: true,
	rename: true,
	drop: true,
	truncate: true,
	duplicate: true,
}

/** Fallback when no live session yet (browser / pre-connect). Mirrors plugin defaults. */
export function defaultCapabilitiesForEngine(
	engine: string,
): EngineCapabilities {
	const e = String(engine).toLowerCase()
	if (e === 'mongodb' || e === 'mongo' || e === 'documentdb') {
		return {
			engine: 'mongodb',
			family: 'DOCUMENT',
			queryLanguage: 'MQL',
			sqlDialect: '',
			supportsSchema: false,
			supportsForeignKeys: false,
			supportsErDiagram: false,
			supportsModelView: true,
			modelViewKind: 'COLLECTION_MAP',
			supportsPagination: true,
			supportsWrite: false,
			supportsCancelQuery: true,
			supportsPooling: true,
			bindsSessionToDatabase: false,
			preferDatabaseFromUri: true,
			connectionKind: 'uri',
			systemDatabases: ['admin', 'local', 'config'],
			nameExample: 'e.g. local-mongo',
			objectLabels: {
				namespace: 'Database',
				container: 'Collection',
				field: 'Field',
			},
			explorer: {
				layout: 'flat',
				multiDatabase: true,
				canCreateDatabase: true,
				databaseLabel: 'Database',
				groupLabel: '',
				objectLabel: 'collections',
				objectLabelSingular: 'collection',
				filterPlaceholder: 'Filter collections…',
			},
			tableAdmin: {
				createStatement: true,
				rename: true,
				drop: true,
				truncate: true,
				duplicate: true,
			},
		}
	}
	if (e === 'sqlite') {
		return {
			engine: 'sqlite',
			family: 'RELATIONAL',
			queryLanguage: 'SQL',
			sqlDialect: 'sqlite',
			supportsSchema: true,
			supportsForeignKeys: true,
			supportsErDiagram: true,
			supportsModelView: true,
			modelViewKind: 'ER',
			supportsPagination: true,
			supportsWrite: false,
			supportsCancelQuery: false,
			supportsPooling: true,
			bindsSessionToDatabase: false,
			preferDatabaseFromUri: false,
			connectionKind: 'file',
			systemDatabases: [],
			nameExample: 'e.g. app-dev.sqlite',
			objectLabels: {
				namespace: 'Schema',
				container: 'Table',
				field: 'Column',
			},
			explorer: {
				layout: 'flat',
				multiDatabase: false,
				canCreateDatabase: false,
				databaseLabel: 'Database',
				groupLabel: '',
				objectLabel: 'tables',
				objectLabelSingular: 'table',
				filterPlaceholder: 'Filter tables…',
			},
			tableAdmin: { ...SQL_ADMIN },
		}
	}
	if (e === 'mysql' || e === 'mariadb') {
		return {
			engine: 'mysql',
			family: 'RELATIONAL',
			queryLanguage: 'SQL',
			sqlDialect: 'mysql',
			supportsSchema: true,
			supportsForeignKeys: true,
			supportsErDiagram: true,
			supportsModelView: true,
			modelViewKind: 'ER',
			supportsPagination: true,
			supportsWrite: false,
			supportsCancelQuery: true,
			supportsPooling: true,
			bindsSessionToDatabase: true,
			preferDatabaseFromUri: false,
			connectionKind: 'host',
			systemDatabases: [],
			nameExample: 'e.g. local-mysql',
			objectLabels: {
				namespace: 'Schema',
				container: 'Table',
				field: 'Column',
			},
			explorer: {
				layout: 'flat',
				multiDatabase: true,
				canCreateDatabase: true,
				databaseLabel: 'Database',
				groupLabel: '',
				objectLabel: 'tables',
				objectLabelSingular: 'table',
				filterPlaceholder: 'Filter tables…',
			},
			tableAdmin: { ...SQL_ADMIN },
		}
	}
	return {
		engine: e || 'postgres',
		family: 'RELATIONAL',
		queryLanguage: 'SQL',
		sqlDialect: 'postgresql',
		supportsSchema: true,
		supportsForeignKeys: true,
		supportsErDiagram: true,
		supportsModelView: true,
		modelViewKind: 'ER',
		supportsPagination: true,
		supportsWrite: false,
		supportsCancelQuery: true,
		supportsPooling: true,
		bindsSessionToDatabase: true,
		preferDatabaseFromUri: false,
		connectionKind: 'host',
		systemDatabases: [],
		nameExample: 'e.g. local-pg',
		objectLabels: { namespace: 'Schema', container: 'Table', field: 'Column' },
		explorer: {
			layout: 'schema-tree',
			multiDatabase: true,
			canCreateDatabase: true,
			databaseLabel: 'Database',
			groupLabel: 'Schema',
			objectLabel: 'tables',
			objectLabelSingular: 'table',
			filterPlaceholder: 'Filter schemas & tables…',
		},
		tableAdmin: { ...SQL_ADMIN },
	}
}

function asBool(v: unknown, fallback: boolean): boolean {
	return typeof v === 'boolean' ? v : fallback
}

function asString(v: unknown, fallback: string): string {
	return typeof v === 'string' ? v : fallback
}

function asStringList(v: unknown, fallback: string[]): string[] {
	if (!Array.isArray(v)) return fallback
	return v.filter((x): x is string => typeof x === 'string')
}

function asConnectionKind(v: unknown, fallback: ConnectionKind): ConnectionKind {
	const s = String(v ?? '')
		.toLowerCase()
		.trim()
	if (s === 'uri' || s === 'file' || s === 'host') return s
	return fallback
}

export function resolveCapabilities(
	engine: string,
	caps?: EngineCapabilities | null,
): EngineCapabilities {
	return caps ?? defaultCapabilitiesForEngine(engine)
}

export function normalizeCapabilities(
	raw: unknown,
	engine: string,
): EngineCapabilities {
	const fallback = defaultCapabilitiesForEngine(engine)
	if (!raw || typeof raw !== 'object') return fallback
	const c = raw as Partial<EngineCapabilities> & {
		explorer?: Partial<ExplorerConfig>
		tableAdmin?: Partial<TableAdminFeatures> & Record<string, unknown>
		table_admin?: Partial<TableAdminFeatures> & Record<string, unknown>
		objectLabels?: Partial<ObjectLabels>
		sql_dialect?: unknown
		prefer_database_from_uri?: unknown
		connection_kind?: unknown
		system_databases?: unknown
		name_example?: unknown
	}
	const adminRaw = (c.tableAdmin ?? c.table_admin ?? {}) as Record<
		string,
		unknown
	>
	return {
		...fallback,
		...c,
		engine: c.engine || fallback.engine,
		sqlDialect: asString(
			c.sqlDialect ?? c.sql_dialect,
			fallback.sqlDialect,
		),
		preferDatabaseFromUri: asBool(
			c.preferDatabaseFromUri ?? c.prefer_database_from_uri,
			fallback.preferDatabaseFromUri,
		),
		connectionKind: asConnectionKind(
			c.connectionKind ?? c.connection_kind,
			fallback.connectionKind,
		),
		systemDatabases: asStringList(
			c.systemDatabases ?? c.system_databases,
			fallback.systemDatabases,
		),
		nameExample: asString(
			c.nameExample ?? c.name_example,
			fallback.nameExample,
		),
		explorer: { ...fallback.explorer, ...(c.explorer ?? {}) },
		tableAdmin: {
			createStatement: asBool(
				adminRaw.createStatement ?? adminRaw.create_statement,
				fallback.tableAdmin.createStatement,
			),
			rename: asBool(adminRaw.rename, fallback.tableAdmin.rename),
			drop: asBool(adminRaw.drop, fallback.tableAdmin.drop),
			truncate: asBool(adminRaw.truncate, fallback.tableAdmin.truncate),
			duplicate: asBool(adminRaw.duplicate, fallback.tableAdmin.duplicate),
		},
		objectLabels: { ...fallback.objectLabels, ...(c.objectLabels ?? {}) },
	}
}

export function presentationFromCaps(caps: EngineCapabilities): {
	layout: 'schema-tree' | 'flat'
	multiDatabase: boolean
	databaseLabel: string
	groupLabel: string
	objectLabel: string
	filterPlaceholder: string
	canCreateDatabase: boolean
} {
	const layout = caps.explorer.layout === 'schema-tree' ? 'schema-tree' : 'flat'
	return {
		layout,
		multiDatabase: caps.explorer.multiDatabase,
		databaseLabel: caps.explorer.databaseLabel,
		groupLabel: caps.explorer.groupLabel,
		objectLabel: caps.explorer.objectLabel,
		filterPlaceholder: caps.explorer.filterPlaceholder,
		canCreateDatabase: caps.explorer.canCreateDatabase,
	}
}

export function familyFromCaps(caps: EngineCapabilities): DataFamily {
	if (caps.family === 'DOCUMENT') return 'nosql'
	if (caps.family === 'KEY_VALUE') return 'redis'
	return 'sql'
}

export function queryLangFromCaps(caps: EngineCapabilities): 'sql' | 'mql' {
	return caps.queryLanguage.toUpperCase() === 'MQL' ? 'mql' : 'sql'
}

export function connectionKindFromCaps(caps: EngineCapabilities): ConnectionKind {
	return asConnectionKind(caps.connectionKind, 'host')
}

/** Tab / diagram title from plugin model-view kind — not from engine name. */
export function modelTabTitle(
	caps: EngineCapabilities,
	database?: string | null,
): string {
	const db = (database ?? '').trim()
	if (caps.supportsErDiagram || caps.modelViewKind === 'ER') {
		return db ? `ER · ${db}` : 'ER diagram'
	}
	return `Model · ${db || 'db'}`
}
