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
}

const SQL_ADMIN: TableAdminFeatures = {
	createStatement: true,
	rename: true,
	drop: true,
	truncate: true,
	duplicate: true,
	viewStructure: true,
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
				canCreateGroup: false,
				canCreateObject: true,
				databaseLabel: 'Database',
				groupLabel: '',
				objectLabel: 'collections',
				objectLabelSingular: 'collection',
				listTitle: 'Collections',
				filterPlaceholder: 'Filter collections…',
			},
			codeFilterPlaceholder: '{ "email": "user@example.com" }',
			tableAdmin: {
				createStatement: true,
				rename: true,
				drop: true,
				truncate: true,
				duplicate: true,
				viewStructure: true,
			},
			exportFormats: ['csv', 'json', 'jsonl'],
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
				canCreateGroup: false,
				canCreateObject: true,
				databaseLabel: 'Database',
				groupLabel: '',
				objectLabel: 'tables',
				objectLabelSingular: 'table',
				listTitle: 'Tables',
				filterPlaceholder: 'Filter tables…',
			},
			codeFilterPlaceholder: '{ "status": "active" } or status = \'active\'',
			tableAdmin: { ...SQL_ADMIN },
			exportFormats: ['csv', 'json', 'jsonl', 'sql'],
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
				canCreateGroup: false,
				canCreateObject: true,
				databaseLabel: 'Database',
				groupLabel: '',
				objectLabel: 'tables',
				objectLabelSingular: 'table',
				listTitle: 'Tables',
				filterPlaceholder: 'Filter tables…',
			},
			codeFilterPlaceholder: '{ "status": "active" } or status = \'active\'',
			tableAdmin: { ...SQL_ADMIN },
			exportFormats: ['csv', 'json', 'jsonl', 'sql'],
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
			canCreateGroup: true,
			canCreateObject: true,
			databaseLabel: 'Database',
			groupLabel: 'Schema',
			objectLabel: 'tables',
			objectLabelSingular: 'table',
			listTitle: 'Tables',
			filterPlaceholder: 'Filter schemas & tables…',
		},
		codeFilterPlaceholder: '{ "status": "active" } or status = \'active\'',
		tableAdmin: { ...SQL_ADMIN },
		exportFormats: ['csv', 'json', 'jsonl', 'sql'],
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

function asConnectionKind(
	v: unknown,
	fallback: ConnectionKind,
): ConnectionKind {
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
		explorer?: Partial<ExplorerConfig> | Record<string, unknown>
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
		sqlDialect: asString(c.sqlDialect ?? c.sql_dialect, fallback.sqlDialect),
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
		codeFilterPlaceholder: asString(
			c.codeFilterPlaceholder ??
				(c as { code_filter_placeholder?: unknown }).code_filter_placeholder,
			fallback.codeFilterPlaceholder,
		),
		exportFormats: asStringList(
			c.exportFormats ??
				(c as { export_formats?: unknown }).export_formats,
			fallback.exportFormats,
		),
		explorer: normalizeExplorer(c.explorer, fallback.explorer),
		tableAdmin: {
			createStatement: asBool(
				adminRaw.createStatement ?? adminRaw.create_statement,
				fallback.tableAdmin.createStatement,
			),
			rename: asBool(adminRaw.rename, fallback.tableAdmin.rename),
			drop: asBool(adminRaw.drop, fallback.tableAdmin.drop),
			truncate: asBool(adminRaw.truncate, fallback.tableAdmin.truncate),
			duplicate: asBool(adminRaw.duplicate, fallback.tableAdmin.duplicate),
			viewStructure: asBool(
				adminRaw.viewStructure ?? adminRaw.view_structure,
				fallback.tableAdmin.viewStructure,
			),
		},
		objectLabels: { ...fallback.objectLabels, ...(c.objectLabels ?? {}) },
	}
}

function normalizeExplorer(
	raw: Partial<ExplorerConfig> | Record<string, unknown> | null | undefined,
	fallback: ExplorerConfig,
): ExplorerConfig {
	const e = (raw ?? {}) as Record<string, unknown>
	return {
		layout: asString(e.layout, fallback.layout),
		multiDatabase: asBool(
			e.multiDatabase ?? e.multi_database,
			fallback.multiDatabase,
		),
		canCreateDatabase: asBool(
			e.canCreateDatabase ?? e.can_create_database,
			fallback.canCreateDatabase,
		),
		canCreateGroup: asBool(
			e.canCreateGroup ?? e.can_create_group,
			fallback.canCreateGroup,
		),
		// Default true: every supported engine can create a leaf object.
		canCreateObject: asBool(
			e.canCreateObject ?? e.can_create_object,
			fallback.canCreateObject !== false,
		),
		databaseLabel: asString(
			e.databaseLabel ?? e.database_label,
			fallback.databaseLabel,
		),
		groupLabel: asString(e.groupLabel ?? e.group_label, fallback.groupLabel),
		objectLabel: asString(
			e.objectLabel ?? e.object_label,
			fallback.objectLabel,
		),
		objectLabelSingular: asString(
			e.objectLabelSingular ?? e.object_label_singular,
			fallback.objectLabelSingular,
		),
		listTitle: asString(e.listTitle ?? e.list_title, fallback.listTitle),
		filterPlaceholder: asString(
			e.filterPlaceholder ?? e.filter_placeholder,
			fallback.filterPlaceholder,
		),
	}
}

export function presentationFromCaps(caps: EngineCapabilities): {
	layout: 'schema-tree' | 'flat'
	multiDatabase: boolean
	databaseLabel: string
	groupLabel: string
	objectLabel: string
	objectLabelSingular: string
	listTitle: string
	filterPlaceholder: string
	canCreateDatabase: boolean
	canCreateGroup: boolean
	canCreateObject: boolean
} {
	const ex = caps.explorer ?? defaultCapabilitiesForEngine(caps.engine).explorer
	const layout = ex.layout === 'schema-tree' ? 'schema-tree' : 'flat'
	const singular =
		(ex.objectLabelSingular || '').trim() ||
		(caps.objectLabels?.container || '').trim().toLowerCase() ||
		'table'
	const listTitle =
		(ex.listTitle || '').trim() ||
		(caps.objectLabels?.container
			? `${caps.objectLabels.container}${caps.objectLabels.container.endsWith('s') ? '' : 's'}`
			: singular.charAt(0).toUpperCase() + singular.slice(1) + 's')
	return {
		layout,
		multiDatabase: !!ex.multiDatabase,
		databaseLabel: ex.databaseLabel || 'Database',
		groupLabel: ex.groupLabel || '',
		objectLabel: ex.objectLabel || `${singular}s`,
		objectLabelSingular: singular,
		listTitle,
		filterPlaceholder:
			ex.filterPlaceholder || `Filter ${ex.objectLabel || singular}…`,
		canCreateDatabase: asBool(ex.canCreateDatabase, false),
		canCreateGroup: asBool(ex.canCreateGroup, false),
		// Prefer explicit false only; missing/undefined → allow create (plugins default true).
		canCreateObject: asBool(ex.canCreateObject, true),
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

export function connectionKindFromCaps(
	caps: EngineCapabilities,
): ConnectionKind {
	return asConnectionKind(caps.connectionKind, 'host')
}

/** Tab / diagram title from plugin model-view kind — not from engine name. */
export function modelTabTitle(
	caps: EngineCapabilities,
	database?: string | null,
	table?: string | null,
): string {
	const object = (table ?? '').trim()
	const db = (database ?? '').trim()
	const label = object || db
	if (caps.supportsErDiagram || caps.modelViewKind === 'ER') {
		return label ? `ER · ${label}` : 'ER diagram'
	}
	return `Model · ${label || 'db'}`
}
