/**
 * Live object explorer catalog — loaded from the engine session (Tauri).
 * Falls back is handled by the caller (mock catalog) when not available.
 */
import { executeEngineQuery, type QueryResult } from "./engines";
import type {
  CatalogObjectKind,
  ColumnDef,
  DatabaseMeta,
  TableMeta,
} from "$lib/types/workspace";

const MONGO_SYSTEM_DBS = new Set(["admin", "local", "config"]);

/** Known FE-only mock database names (never come from a real Mongo server seed). */
const MONGO_MOCK_DB_NAMES = new Set(["app", "analytics"]);

/** Prefer live app seed over legacy local names when both exist. */
const MONGO_LEGACY_PREFERRED = new Set(["mystique_dev", "test", "app"]);

function cellStr(v: unknown): string {
  if (v == null) return "";
  if (typeof v === "string") return v;
  if (typeof v === "number" || typeof v === "boolean") return String(v);
  try {
    return JSON.stringify(v);
  } catch {
    return String(v);
  }
}

function cellNum(v: unknown): number | null {
  if (typeof v === "number" && Number.isFinite(v)) return v;
  if (typeof v === "string" && v.trim() !== "") {
    const n = Number(v);
    if (Number.isFinite(n)) return n;
  }
  return null;
}

function colIndex(result: QueryResult, name: string): number {
  return result.columns.findIndex(
    (c) => c.name.toLowerCase() === name.toLowerCase(),
  );
}

function emptyLeaf(
  database: string,
  schema: string,
  name: string,
  kind: CatalogObjectKind,
  rowCount = 0,
): TableMeta {
  const id = schema ? `${database}.${schema}.${name}` : `${database}.${name}`;
  return {
    id,
    database,
    schema,
    name,
    kind,
    rowCount,
    columns: [],
    rows: [],
  };
}

/** Prefer seeded app DB, then first non-system name. */
export function preferDatabase(names: string[], preferred?: string | null): string {
  const pref = preferred?.trim() || "";
  // Upgrade stale preferred names when the 10k seed DB is present.
  if (pref && names.includes(pref)) {
    if (MONGO_LEGACY_PREFERRED.has(pref) && names.includes("mystique")) {
      return "mystique";
    }
    return pref;
  }
  if (names.includes("mystique")) return "mystique";
  const nonSystem = names.filter((n) => !MONGO_SYSTEM_DBS.has(n));
  return nonSystem[0] ?? names[0] ?? "";
}

/**
 * True when the explorer is still showing FE mock shells (pre-live-catalog era).
 * Used to force a live reload for old workspace sessions.
 */
export function isMockCatalog(engine: string, list: DatabaseMeta[] | undefined | null): boolean {
  if (!list || list.length === 0) return false;
  const e = engine.toLowerCase();
  if (e === "mongodb") {
    const names = new Set(list.map((d) => d.name));
    // Classic mock: app + analytics (+ admin)
    if (names.has("app") && names.has("analytics")) return true;
    // No real user DBs from Docker seed / only mock leaves
    const user = list.filter((d) => !MONGO_SYSTEM_DBS.has(d.name));
    if (user.length > 0 && user.every((d) => MONGO_MOCK_DB_NAMES.has(d.name))) {
      return true;
    }
  }
  return false;
}

/**
 * Load databases + collection/table shells for the explorer.
 * Mongo: listDatabases + listCollections per user DB.
 * SQL: list tables in the current connection database.
 * MySQL: list tables per schema (database), excluding system schemas.
 */
export async function loadLiveCatalog(
  sessionId: string,
  engine: string,
  opts?: { preferredDatabase?: string | null },
): Promise<DatabaseMeta[]> {
  const e = engine.toLowerCase();
  if (e === "mongodb") {
    return loadMongoCatalog(sessionId, opts?.preferredDatabase);
  }
  if (e === "mysql" || e === "mariadb") {
    return loadMysqlCatalog(sessionId, opts?.preferredDatabase);
  }
  if (e === "postgres" || e === "postgresql" || e === "sqlite") {
    return loadSqlCatalog(sessionId, e);
  }
  throw new Error(`Live catalog not implemented for engine \`${engine}\``);
}

async function loadMongoCatalog(
  sessionId: string,
  preferredDatabase?: string | null,
): Promise<DatabaseMeta[]> {
  const dbsResult = await executeEngineQuery({
    sessionId,
    text: "listDatabases",
    maxRows: 200,
  });
  const nameIdx = Math.max(0, colIndex(dbsResult, "name"));
  const allNames = dbsResult.rows
    .map((r) => cellStr(r[nameIdx]))
    .filter((n) => n.length > 0);

  // Show app DBs first; keep system DBs at the end (collapsed usefulness).
  const userDbs = allNames.filter((n) => !MONGO_SYSTEM_DBS.has(n));
  const systemDbs = allNames.filter((n) => MONGO_SYSTEM_DBS.has(n));
  const ordered = [...userDbs, ...systemDbs];

  const out: DatabaseMeta[] = [];
  for (const dbName of ordered) {
    const isSystem = MONGO_SYSTEM_DBS.has(dbName);
    let tables: TableMeta[] = [];
    if (!isSystem) {
      try {
        const collResult = await executeEngineQuery({
          sessionId,
          text: "listCollections",
          database: dbName,
          maxRows: 500,
        });
        const cName = Math.max(0, colIndex(collResult, "name"));
        const cCount = colIndex(collResult, "estimatedCount");
        tables = collResult.rows
          .map((r) => {
            const name = cellStr(r[cName]);
            if (!name) return null;
            const count = cCount >= 0 ? cellNum(r[cCount]) : null;
            return emptyLeaf(dbName, "", name, "collection", count ?? 0);
          })
          .filter((t): t is TableMeta => t != null);
      } catch {
        tables = [];
      }
    }
    out.push({
      name: dbName,
      meta: isSystem ? "system" : tables.length ? `${tables.length} collections` : undefined,
      tables,
    });
  }

  // Ensure preferred DB exists in list even if empty (edge cases).
  const pref = preferDatabase(
    out.map((d) => d.name),
    preferredDatabase,
  );
  if (pref && !out.some((d) => d.name === pref)) {
    out.unshift({ name: pref, tables: [] });
  }

  return out;
}

async function loadSqlCatalog(
  sessionId: string,
  engine: string,
): Promise<DatabaseMeta[]> {
  const isSqlite = engine === "sqlite";
  const sql = isSqlite
    ? `SELECT name AS table_name, type AS table_type FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name`
    : `SELECT table_schema, table_name, table_type
       FROM information_schema.tables
       WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
       ORDER BY table_schema, table_name`;

  const result = await executeEngineQuery({
    sessionId,
    text: sql,
    maxRows: 2000,
  });

  const dbLabel = isSqlite ? "main" : "current";
  const schemaIdx = colIndex(result, "table_schema");
  const nameIdx = Math.max(
    0,
    colIndex(result, "table_name") >= 0
      ? colIndex(result, "table_name")
      : colIndex(result, "name"),
  );
  const typeIdx = colIndex(result, "table_type") >= 0
    ? colIndex(result, "table_type")
    : colIndex(result, "type");

  const tables: TableMeta[] = [];
  for (const row of result.rows) {
    const name = cellStr(row[nameIdx]);
    if (!name) continue;
    const schema = schemaIdx >= 0 ? cellStr(row[schemaIdx]) : "";
    const rawType = typeIdx >= 0 ? cellStr(row[typeIdx]).toLowerCase() : "table";
    const kind: CatalogObjectKind =
      rawType.includes("view") ? "view" : "table";
    tables.push(emptyLeaf(dbLabel, schema, name, kind, 0));
  }

  return [{ name: dbLabel, meta: "live", tables }];
}

/** MySQL: schema == database. Group tables under each user database. */
async function loadMysqlCatalog(
  sessionId: string,
  preferredDatabase?: string | null,
): Promise<DatabaseMeta[]> {
  // TABLE_ROWS is InnoDB's approximate row count (cheap; good enough for the explorer).
  const sql = `SELECT table_schema, table_name, table_type,
      COALESCE(table_rows, 0) AS table_rows
    FROM information_schema.tables
    WHERE table_schema NOT IN ('mysql', 'information_schema', 'performance_schema', 'sys')
    ORDER BY table_schema, table_name`;

  const result = await executeEngineQuery({
    sessionId,
    text: sql,
    maxRows: 5000,
  });

  const schemaIdx = Math.max(0, colIndex(result, "table_schema"));
  const nameIdx = Math.max(0, colIndex(result, "table_name"));
  const typeIdx = colIndex(result, "table_type");
  const rowsIdx = colIndex(result, "table_rows");

  const byDb = new Map<string, TableMeta[]>();
  for (const row of result.rows) {
    const dbName = cellStr(row[schemaIdx]);
    const name = cellStr(row[nameIdx]);
    if (!dbName || !name) continue;
    const rawType = typeIdx >= 0 ? cellStr(row[typeIdx]).toLowerCase() : "table";
    const kind: CatalogObjectKind = rawType.includes("view") ? "view" : "table";
    const approx =
      rowsIdx >= 0 ? Math.max(0, Math.floor(cellNum(row[rowsIdx]) ?? 0)) : 0;
    // Views often report 0/NULL — leave 0 rather than running COUNT(*).
    const leaf = emptyLeaf(dbName, dbName, name, kind, approx);
    const list = byDb.get(dbName) ?? [];
    list.push(leaf);
    byDb.set(dbName, list);
  }

  // Ensure preferred DB appears even if empty (edge cases).
  const pref = preferredDatabase?.trim();
  if (pref && !byDb.has(pref)) {
    byDb.set(pref, []);
  }

  // Prefer mystique / preferred first.
  const names = [...byDb.keys()].sort((a, b) => {
    if (pref && a === pref) return -1;
    if (pref && b === pref) return 1;
    if (a === "mystique") return -1;
    if (b === "mystique") return 1;
    return a.localeCompare(b);
  });

  return names.map((name) => {
    const tables = byDb.get(name) ?? [];
    return {
      name,
      meta: tables.length ? `${tables.length} tables` : "empty",
      tables,
    };
  });
}

/** Fetch a page of rows for a table/collection and merge into TableMeta. */
export async function loadTablePreview(
  sessionId: string,
  engine: string,
  table: TableMeta,
  maxRows = 100,
): Promise<TableMeta> {
  const e = engine.toLowerCase();
  let result: QueryResult;

  if (e === "mongodb") {
    result = await executeEngineQuery({
      sessionId,
      text: "{}",
      collection: table.name,
      database: table.database,
      maxRows,
    });
  } else if (e === "mysql" || e === "mariadb") {
    // MySQL uses backticks; schema == database name when present.
    const tick = (id: string) => `\`${id.replace(/`/g, "``")}\``;
    const qualified =
      table.schema && table.schema.length > 0
        ? `${tick(table.schema)}.${tick(table.name)}`
        : table.database
          ? `${tick(table.database)}.${tick(table.name)}`
          : tick(table.name);
    // Request one extra row so the BE can set truncated when more data exists.
    result = await executeEngineQuery({
      sessionId,
      text: `SELECT * FROM ${qualified} LIMIT ${maxRows + 1}`,
      maxRows,
    });
  } else {
    const qualified =
      table.schema && table.schema.length > 0
        ? `"${table.schema.replace(/"/g, '""')}"."${table.name.replace(/"/g, '""')}"`
        : `"${table.name.replace(/"/g, '""')}"`;
    const sql =
      e === "sqlite"
        ? `SELECT * FROM "${table.name.replace(/"/g, '""')}" LIMIT ${maxRows + 1}`
        : `SELECT * FROM ${qualified} LIMIT ${maxRows + 1}`;
    result = await executeEngineQuery({
      sessionId,
      text: sql,
      maxRows,
    });
  }

  return queryResultToTableMeta(table, result);
}

export function queryResultToTableMeta(
  base: TableMeta,
  result: QueryResult,
): TableMeta {
  const columns: ColumnDef[] = result.columns.map((c) => ({
    name: c.name,
    type: c.dataType ?? "unknown",
    pk: c.name === "_id" || c.name === "id",
  }));

  const rows: Record<string, unknown>[] = result.rows.map((cells) => {
    const obj: Record<string, unknown> = {};
    result.columns.forEach((c, i) => {
      const v = cells[i];
      // Keep nested objects readable in the grid
      if (v != null && typeof v === "object") {
        try {
          obj[c.name] = JSON.stringify(v);
        } catch {
          obj[c.name] = v;
        }
      } else {
        obj[c.name] = v ?? null;
      }
    });
    return obj;
  });

  // Never shrink a better catalog estimate (e.g. MySQL TABLE_ROWS / Mongo estimatedCount)
  // just because the data preview is a limited page.
  const pageCount = result.rowCount;
  const rowCount = Math.max(base.rowCount ?? 0, pageCount);

  return {
    ...base,
    columns: columns.length > 0 ? columns : base.columns,
    rows,
    rowCount,
  };
}
