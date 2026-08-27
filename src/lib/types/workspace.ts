/** Workspace (connected session) types — UI + mock data for now. */
import {
	defaultCapabilitiesForEngine,
	presentationFromCaps,
	type EngineCapabilities,
} from './engineCaps'

/** Plugin-owned FK (or equivalent) target for grid jump-to-row. */
export type ColumnReference = {
	database?: string | null
	schema?: string | null
	table: string
	column: string
}

export type ColumnDef = {
	name: string
	type: string
	nullable?: boolean
	pk?: boolean
	/**
	 * Discrete options for enum / set-like types (MySQL ENUM, Postgres enum).
	 * When set, the grid/inspector use a dropdown instead of free text.
	 */
	enumValues?: string[]
	/**
	 * When set by the engine plugin, the grid shows a jump control to this
	 * referenced table/column for non-null cell values.
	 */
	ref?: ColumnReference
}

/**
 * Object tree layout *within* the selected database.
 * Database selection is always a top-level control (except single-file SQLite).
 */
export type CatalogLayout = 'schema-tree' | 'flat'

/** Object kind for iconography / labels. */
export type CatalogObjectKind = 'table' | 'collection' | 'view'

export type TableMeta = {
	/**
	 * Unique within a connection:
	 * - schema-tree: `database.schema.name`
	 * - flat: `database.name`
	 */
	id: string
	/** Owning database (server catalog name). */
	database: string
	/**
	 * Namespace inside the database:
	 * - Postgres: schema (`public`, `auth`)
	 * - MySQL / SQLite / Mongo: empty
	 */
	schema: string
	name: string
	kind: CatalogObjectKind
	rowCount: number
	columns: ColumnDef[]
	rows: Record<string, unknown>[]
}

export type DatabaseMeta = {
	name: string
	/** Optional size / owner label for the picker. */
	meta?: string
	tables: TableMeta[]
}

/** Main workspace tab: query canvas, table data, ER / model, or object structure. */
export type WorkspaceTabKind = 'query' | 'table' | 'model' | 'structure'

export type WorkspaceTab = {
	id: string
	connectionId: string
	kind: WorkspaceTabKind
	/** Present when kind === "table", "structure", or a table-scoped "model" tab. */
	tableId?: string
	/**
	 * Database name the model/ER tab was opened for (kind === "model").
	 * Empty string for single-db engines when unknown.
	 */
	modelDatabase?: string
	title: string
	/**
	 * Linked `saved_queries.id` when this query tab is persisted in the library.
	 * Null/undefined = never saved (or table/model tab).
	 */
	savedQueryId?: string | null
	/**
	 * Unsaved state: query draft differs from saved body, or table grid has edits.
	 * Shown as a dirty indicator on the tab chrome.
	 */
	dirty?: boolean
}

/** @deprecated Use WorkspaceTab */
export type OpenTableTab = WorkspaceTab

export type SelectedRow = {
	connectionId: string
	tableId: string
	rowIndex: number
}

/** Explorer layout + copy for an engine (within one database). */
export type CatalogPresentation = {
	layout: CatalogLayout
	/** Whether the engine exposes multiple server databases. */
	multiDatabase: boolean
	/** Label for the database control */
	databaseLabel: string
	/** Group node label (schema-tree only) */
	groupLabel: string
	/** Leaf object plural */
	objectLabel: string
	filterPlaceholder: string
	/** Create-database is meaningful */
	canCreateDatabase: boolean
}

export function catalogPresentation(
	engine: string,
	caps?: EngineCapabilities | null,
): CatalogPresentation {
	return presentationFromCaps(caps ?? defaultCapabilitiesForEngine(engine))
}
