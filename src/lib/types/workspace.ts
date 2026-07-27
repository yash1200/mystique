/** Workspace (connected session) types — UI + mock data for now. */

export type ColumnDef = {
  name: string;
  type: string;
  nullable?: boolean;
  pk?: boolean;
};

/**
 * Object tree layout *within* the selected database.
 * Database selection is always a top-level control (except single-file SQLite).
 */
export type CatalogLayout = "schema-tree" | "flat";

/** Object kind for iconography / labels. */
export type CatalogObjectKind = "table" | "collection" | "view";

export type TableMeta = {
  /**
   * Unique within a connection:
   * - schema-tree: `database.schema.name`
   * - flat: `database.name`
   */
  id: string;
  /** Owning database (server catalog name). */
  database: string;
  /**
   * Namespace inside the database:
   * - Postgres: schema (`public`, `auth`)
   * - MySQL / SQLite / Mongo: empty
   */
  schema: string;
  name: string;
  kind: CatalogObjectKind;
  rowCount: number;
  columns: ColumnDef[];
  rows: Record<string, unknown>[];
};

export type DatabaseMeta = {
  name: string;
  /** Optional size / owner label for the picker. */
  meta?: string;
  tables: TableMeta[];
};

/** Main workspace tab: query canvas or table data. */
export type WorkspaceTabKind = "query" | "table";

export type WorkspaceTab = {
  id: string;
  connectionId: string;
  kind: WorkspaceTabKind;
  /** Present when kind === "table". */
  tableId?: string;
  title: string;
};

/** @deprecated Use WorkspaceTab */
export type OpenTableTab = WorkspaceTab;

export type SelectedRow = {
  connectionId: string;
  tableId: string;
  rowIndex: number;
};

/** Explorer layout + copy for an engine (within one database). */
export type CatalogPresentation = {
  layout: CatalogLayout;
  /** Whether the engine exposes multiple server databases. */
  multiDatabase: boolean;
  /** Label for the database control */
  databaseLabel: string;
  /** Group node label (schema-tree only) */
  groupLabel: string;
  /** Leaf object plural */
  objectLabel: string;
  filterPlaceholder: string;
  /** Create-database is meaningful */
  canCreateDatabase: boolean;
};

export function catalogPresentation(engine: string): CatalogPresentation {
  const e = String(engine).toLowerCase();
  if (e === "postgres" || e === "postgresql") {
    return {
      layout: "schema-tree",
      multiDatabase: true,
      databaseLabel: "Database",
      groupLabel: "Schema",
      objectLabel: "tables",
      filterPlaceholder: "Filter schemas & tables…",
      canCreateDatabase: true,
    };
  }
  if (e === "mongodb") {
    return {
      layout: "flat",
      multiDatabase: true,
      databaseLabel: "Database",
      groupLabel: "",
      objectLabel: "collections",
      filterPlaceholder: "Filter collections…",
      canCreateDatabase: true,
    };
  }
  if (e === "mysql" || e === "mariadb") {
    return {
      layout: "flat",
      multiDatabase: true,
      databaseLabel: "Database",
      groupLabel: "",
      objectLabel: "tables",
      filterPlaceholder: "Filter tables…",
      canCreateDatabase: true,
    };
  }
  if (e === "sqlite") {
    return {
      layout: "flat",
      multiDatabase: false,
      databaseLabel: "Database",
      groupLabel: "",
      objectLabel: "tables",
      filterPlaceholder: "Filter tables…",
      canCreateDatabase: false,
    };
  }
  return {
    layout: "flat",
    multiDatabase: true,
    databaseLabel: "Database",
    groupLabel: "",
    objectLabel: "tables",
    filterPlaceholder: "Filter tables…",
    canCreateDatabase: true,
  };
}
