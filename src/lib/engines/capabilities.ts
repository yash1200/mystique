/**
 * Pure helpers over `EngineCapabilities`. No engine ids here — every function
 * takes capabilities and returns chrome. Per-engine defaults live in
 * `engines/<engine>/capabilities.ts`; resolution lives in `engines/registry`.
 */
import type {
	ConnectionKind,
	DataFamily,
	EngineCapabilities,
	ExplorerConfig,
	ObjectLabels,
	SupportDepth,
	TableAdminFeatures,
} from '#core/contracts'

/** Table admin surface shared by every SQL engine. */
export const SQL_ADMIN: TableAdminFeatures = {
	createStatement: true,
	rename: true,
	drop: true,
	truncate: true,
	duplicate: true,
	viewStructure: true,
}

export const SQL_OBJECT_LABELS: ObjectLabels = {
	namespace: 'Schema',
	container: 'Table',
	field: 'Column',
}

export const SQL_CODE_FILTER_PLACEHOLDER =
	'{ "status": "active" } or status = \'active\''

export const SQL_EXPORT_FORMATS = ['csv', 'json', 'jsonl', 'sql']

/**
 * Baseline relational capabilities. Postgres / MySQL / SQLite override only the
 * handful of fields where they actually differ.
 */
export function relationalCapabilities(
	over: Partial<EngineCapabilities> & { engine: string; sqlDialect: string },
): EngineCapabilities {
	const explorer: ExplorerConfig = {
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
		...(over.explorer ?? {}),
	}
	return {
		family: 'RELATIONAL',
		queryLanguage: 'SQL',
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
		nameExample: 'e.g. local-db',
		objectLabels: { ...SQL_OBJECT_LABELS },
		codeFilterPlaceholder: SQL_CODE_FILTER_PLACEHOLDER,
		tableAdmin: { ...SQL_ADMIN },
		exportFormats: [...SQL_EXPORT_FORMATS],
		supportsServerMetrics: true,
		supportsTerminateSession: true,
		supportsExplorer: true,
		supportDepth: 'operate',
		...over,
		explorer,
	}
}

export function parseSupportDepth(v: unknown, fallback: SupportDepth): SupportDepth {
	const s = String(v ?? '')
		.toLowerCase()
		.trim()
	if (s === 'operate' || s === 'browse' || s === 'connect') return s
	return fallback
}

/** Clamp chrome flags to supportDepth. `operate` leaves plugin flags as-is. */
export function applySupportDepth(caps: EngineCapabilities): EngineCapabilities {
	const depth = parseSupportDepth(caps.supportDepth, 'operate')
	if (depth === 'operate') {
		return { ...caps, supportDepth: 'operate', supportsExplorer: true }
	}
	const browse: EngineCapabilities = {
		...caps,
		supportDepth: 'browse',
		supportsExplorer: true,
		supportsErDiagram: false,
		supportsModelView: false,
		modelViewKind: 'NONE',
		supportsServerMetrics: false,
		supportsTerminateSession: false,
		tableAdmin: {
			...caps.tableAdmin,
			createStatement: false,
			rename: false,
			drop: false,
			truncate: false,
			duplicate: false,
		},
		explorer: {
			...caps.explorer,
			canCreateDatabase: false,
			canCreateGroup: false,
			canCreateObject: false,
		},
	}
	if (depth === 'browse') return browse
	return {
		...browse,
		supportDepth: 'connect',
		supportsExplorer: false,
		tableAdmin: { ...browse.tableAdmin, viewStructure: false },
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

/** Merge raw plugin JSON (snake_case tolerated) onto a known-good fallback. */
export function mergeCapabilities(
	raw: unknown,
	fallback: EngineCapabilities,
): EngineCapabilities {
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
	const merged: EngineCapabilities = {
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
			c.exportFormats ?? (c as { export_formats?: unknown }).export_formats,
			fallback.exportFormats,
		),
		supportsServerMetrics: asBool(
			c.supportsServerMetrics ??
				(c as { supports_server_metrics?: unknown }).supports_server_metrics,
			fallback.supportsServerMetrics,
		),
		supportsTerminateSession: asBool(
			c.supportsTerminateSession ??
				(c as { supports_terminate_session?: unknown })
					.supports_terminate_session,
			fallback.supportsTerminateSession,
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
		supportsExplorer: asBool(
			c.supportsExplorer ??
				(c as { supports_explorer?: unknown }).supports_explorer,
			fallback.supportsExplorer,
		),
		supportDepth: parseSupportDepth(
			c.supportDepth ?? (c as { support_depth?: unknown }).support_depth,
			fallback.supportDepth,
		),
	}
	return applySupportDepth(merged)
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
	const ex = caps.explorer
	const layout = ex?.layout === 'schema-tree' ? 'schema-tree' : 'flat'
	const singular =
		(ex?.objectLabelSingular || '').trim() ||
		(caps.objectLabels?.container || '').trim().toLowerCase() ||
		'table'
	const listTitle =
		(ex?.listTitle || '').trim() ||
		(caps.objectLabels?.container
			? `${caps.objectLabels.container}${caps.objectLabels.container.endsWith('s') ? '' : 's'}`
			: singular.charAt(0).toUpperCase() + singular.slice(1) + 's')
	return {
		layout,
		multiDatabase: !!ex?.multiDatabase,
		databaseLabel: ex?.databaseLabel || 'Database',
		groupLabel: ex?.groupLabel || '',
		objectLabel: ex?.objectLabel || `${singular}s`,
		objectLabelSingular: singular,
		listTitle,
		filterPlaceholder:
			ex?.filterPlaceholder || `Filter ${ex?.objectLabel || singular}…`,
		canCreateDatabase: asBool(ex?.canCreateDatabase, false),
		canCreateGroup: asBool(ex?.canCreateGroup, false),
		// Prefer explicit false only; missing/undefined → allow create (plugins default true).
		canCreateObject: asBool(ex?.canCreateObject, true),
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

/** Model-view label used by the engine picker copy. */
export function modelViewLabelFromCaps(caps: EngineCapabilities): string {
	if (caps.supportsErDiagram || caps.modelViewKind === 'ER') return 'ER diagram'
	if (caps.modelViewKind === 'COLLECTION_MAP') return 'Collection map'
	return 'Model'
}
