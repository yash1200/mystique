/** Catalog shapes the explorer, grid and tabs all read. */
import type { ColumnReference } from '../ipc/bindings.gen'

/** Plugin-owned FK (or equivalent) target for grid jump-to-row. */
export type { ColumnReference }

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
	/** False until COUNT finishes. Pager treats a full page as “maybe more”. */
	totalKnown?: boolean
	/** True when the last page filled maxRows (more rows may exist). */
	truncated?: boolean
	columns: ColumnDef[]
	rows: Record<string, unknown>[]
}

export type DatabaseMeta = {
	name: string
	/** Optional size / owner label for the picker. */
	meta?: string
	tables: TableMeta[]
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
