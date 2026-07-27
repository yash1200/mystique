/**
 * Build a live ER / model diagram from an open engine session.
 * Common path for all engines: relational FK graph when available;
 * document engines get a collection map (honest, no fake FKs).
 */
import { executeEngineQuery, type QueryResult } from "./engines";
import type {
  DataFamily,
  ErColumn,
  ErDiagramState,
  ErEdge,
  ErNode,
} from "$lib/types/er";
import { ER_GRID } from "$lib/types/er";

const TABLE_W = 280;
const HEADER_H = 40;
const ROW_H = 28;
const GAP_X = ER_GRID * 3;
const GAP_Y = ER_GRID * 3;

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

function colIndex(result: QueryResult, name: string): number {
  return result.columns.findIndex(
    (c) => c.name.toLowerCase() === name.toLowerCase(),
  );
}

function nodeId(schema: string, name: string): string {
  const s = schema?.trim();
  return s ? `n_${s}__${name}` : `n_${name}`;
}

function colId(schema: string, table: string, column: string): string {
  const s = schema?.trim();
  return s ? `c_${s}__${table}__${column}` : `c_${table}__${column}`;
}

function layoutNodes(nodes: ErNode[]): ErNode[] {
  if (nodes.length === 0) return nodes;
  const cols = Math.max(1, Math.ceil(Math.sqrt(nodes.length)));
  return nodes.map((n, i) => {
    const col = i % cols;
    const row = Math.floor(i / cols);
    // Stack rows by max height of previous row-ish: fixed estimate
    const estH = HEADER_H + Math.max(1, n.columns.length) * ROW_H + 48;
    return {
      ...n,
      x: ER_GRID * 2 + col * (TABLE_W + GAP_X),
      y: ER_GRID * 2 + row * (Math.max(estH, 160) + GAP_Y),
    };
  });
}

export function familyForEngine(engine: string): DataFamily {
  const e = engine.toLowerCase();
  if (e === "mongodb" || e === "mongo" || e === "documentdb") return "nosql";
  if (e === "redis") return "redis";
  return "sql";
}

export type LiveErLoadResult = {
  diagram: ErDiagramState;
  title: string;
  /** Human note (e.g. "4 FK relationships"). */
  summary: string;
};

/** Load full database structure diagram for the active connection + database. */
export async function loadLiveErDiagram(
  sessionId: string,
  engine: string,
  database: string | null | undefined,
): Promise<LiveErLoadResult> {
  const e = engine.toLowerCase();
  const db = (database ?? "").trim();

  if (e === "mysql" || e === "mariadb") {
    return loadMysqlEr(sessionId, db || null);
  }
  if (e === "postgres" || e === "postgresql") {
    return loadPostgresEr(sessionId, db || null);
  }
  if (e === "sqlite") {
    return loadSqliteEr(sessionId);
  }
  if (e === "mongodb" || e === "mongo") {
    return loadMongoEr(sessionId, db || null);
  }
  // Generic SQL-ish fallback
  return loadMysqlEr(sessionId, db || null);
}

async function loadMysqlEr(
  sessionId: string,
  database: string | null,
): Promise<LiveErLoadResult> {
  // Resolve schema: explicit DB, else DATABASE()
  let schema = database;
  if (!schema) {
    const r = await executeEngineQuery({
      sessionId,
      text: "SELECT DATABASE() AS db",
      maxRows: 1,
    });
    schema = cellStr(r.rows[0]?.[0]) || "mystique";
  }

  const colsResult = await executeEngineQuery({
    sessionId,
    text: `
      SELECT table_name, column_name, column_type, is_nullable, column_key
      FROM information_schema.columns
      WHERE table_schema = '${schema.replace(/'/g, "''")}'
      ORDER BY table_name, ordinal_position
    `,
    maxRows: 5000,
  });

  const fkResult = await executeEngineQuery({
    sessionId,
    text: `
      SELECT table_name, column_name, referenced_table_name, referenced_column_name, constraint_name
      FROM information_schema.key_column_usage
      WHERE table_schema = '${schema.replace(/'/g, "''")}'
        AND referenced_table_name IS NOT NULL
      ORDER BY table_name, ordinal_position
    `,
    maxRows: 2000,
  });

  const diagram = buildFromColumnRows(
    "sql",
    colsResult,
    {
      table: "table_name",
      column: "column_name",
      type: "column_type",
      nullable: "is_nullable",
      key: "column_key",
      schema: null,
    },
    schema,
  );
  addFkEdges(diagram, fkResult, {
    fromTable: "table_name",
    fromColumn: "column_name",
    toTable: "referenced_table_name",
    toColumn: "referenced_column_name",
    fromSchema: null,
    toSchema: null,
  }, schema);

  return {
    diagram: { ...diagram, nodes: layoutNodes(diagram.nodes) },
    title: `ER · ${schema}`,
    summary: `${diagram.nodes.length} tables · ${diagram.edges.length} relationships`,
  };
}

async function loadPostgresEr(
  sessionId: string,
  _database: string | null,
): Promise<LiveErLoadResult> {
  const colsResult = await executeEngineQuery({
    sessionId,
    text: `
      SELECT table_schema, table_name, column_name, data_type, is_nullable,
             CASE WHEN EXISTS (
               SELECT 1
               FROM information_schema.table_constraints tc
               JOIN information_schema.key_column_usage k
                 ON k.constraint_name = tc.constraint_name
                AND k.table_schema = tc.table_schema
               WHERE tc.constraint_type = 'PRIMARY KEY'
                 AND tc.table_schema = c.table_schema
                 AND tc.table_name = c.table_name
                 AND k.column_name = c.column_name
             ) THEN 'PRI' ELSE '' END AS column_key
      FROM information_schema.columns c
      WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
      ORDER BY table_schema, table_name, ordinal_position
    `,
    maxRows: 5000,
  });

  const fkResult = await executeEngineQuery({
    sessionId,
    text: `
      SELECT
        kcu.table_schema AS from_schema,
        kcu.table_name AS from_table,
        kcu.column_name AS from_column,
        ccu.table_schema AS to_schema,
        ccu.table_name AS to_table,
        ccu.column_name AS to_column
      FROM information_schema.table_constraints tc
      JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
       AND tc.table_schema = kcu.table_schema
      JOIN information_schema.constraint_column_usage ccu
        ON ccu.constraint_name = tc.constraint_name
       AND ccu.table_schema = tc.table_schema
      WHERE tc.constraint_type = 'FOREIGN KEY'
      ORDER BY kcu.table_schema, kcu.table_name
    `,
    maxRows: 2000,
  });

  const diagram = buildFromColumnRows(
    "sql",
    colsResult,
    {
      table: "table_name",
      column: "column_name",
      type: "data_type",
      nullable: "is_nullable",
      key: "column_key",
      schema: "table_schema",
    },
    null,
  );
  addFkEdges(diagram, fkResult, {
    fromTable: "from_table",
    fromColumn: "from_column",
    toTable: "to_table",
    toColumn: "to_column",
    fromSchema: "from_schema",
    toSchema: "to_schema",
  }, null);

  const label =
    diagram.nodes.length > 0
      ? `ER · ${[...new Set(diagram.nodes.map((n) => n.name.split(".")[0] ?? n.name))].length} schemas`
      : "ER · current database";

  return {
    diagram: { ...diagram, nodes: layoutNodes(diagram.nodes) },
    title: label,
    summary: `${diagram.nodes.length} tables · ${diagram.edges.length} relationships`,
  };
}

async function loadSqliteEr(sessionId: string): Promise<LiveErLoadResult> {
  // table-valued pragma (SQLite 3.16+)
  let colsResult: QueryResult;
  try {
    colsResult = await executeEngineQuery({
      sessionId,
      text: `
        SELECT m.name AS table_name, p.name AS column_name, p.type AS data_type,
               CASE WHEN p."notnull" = 0 THEN 'YES' ELSE 'NO' END AS is_nullable,
               CASE WHEN p.pk > 0 THEN 'PRI' ELSE '' END AS column_key
        FROM sqlite_master m
        JOIN pragma_table_info(m.name) p
        WHERE m.type = 'table' AND m.name NOT LIKE 'sqlite_%'
        ORDER BY m.name, p.cid
      `,
      maxRows: 5000,
    });
  } catch {
    // Fallback: table list only
    const tables = await executeEngineQuery({
      sessionId,
      text: `SELECT name AS table_name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name`,
      maxRows: 500,
    });
    const nodes: ErNode[] = tables.rows.map((r, i) => {
      const name = cellStr(r[0]);
      return {
        id: nodeId("", name),
        kind: "table" as const,
        name,
        columns: [
          { id: colId("", name, "rowid"), name: "rowid", dataType: "INTEGER", pk: true },
        ],
        x: 0,
        y: i * 100,
      };
    });
    return {
      diagram: { family: "sql", nodes: layoutNodes(nodes), edges: [] },
      title: "ER · main",
      summary: `${nodes.length} tables`,
    };
  }

  const diagram = buildFromColumnRows(
    "sql",
    colsResult,
    {
      table: "table_name",
      column: "column_name",
      type: "data_type",
      nullable: "is_nullable",
      key: "column_key",
      schema: null,
    },
    null,
  );

  try {
    const fkResult = await executeEngineQuery({
      sessionId,
      text: `
        SELECT m.name AS from_table, f."from" AS from_column,
               f."table" AS to_table, f."to" AS to_column
        FROM sqlite_master m
        JOIN pragma_foreign_key_list(m.name) f
        WHERE m.type = 'table' AND m.name NOT LIKE 'sqlite_%'
      `,
      maxRows: 2000,
    });
    addFkEdges(diagram, fkResult, {
      fromTable: "from_table",
      fromColumn: "from_column",
      toTable: "to_table",
      toColumn: "to_column",
      fromSchema: null,
      toSchema: null,
    }, null);
  } catch {
    /* no FK pragma support */
  }

  return {
    diagram: { ...diagram, nodes: layoutNodes(diagram.nodes) },
    title: "ER · main",
    summary: `${diagram.nodes.length} tables · ${diagram.edges.length} relationships`,
  };
}

async function loadMongoEr(
  sessionId: string,
  database: string | null,
): Promise<LiveErLoadResult> {
  let dbName = database;
  if (!dbName) {
    const dbs = await executeEngineQuery({
      sessionId,
      text: "listDatabases",
      maxRows: 100,
    });
    const names = dbs.rows.map((r) => cellStr(r[0])).filter(Boolean);
    dbName =
      names.find((n) => n === "mystique") ??
      names.find((n) => !["admin", "local", "config"].includes(n)) ??
      names[0] ??
      "test";
  }

  const collResult = await executeEngineQuery({
    sessionId,
    text: "listCollections",
    database: dbName,
    maxRows: 500,
  });
  const nameIdx = Math.max(0, colIndex(collResult, "name"));

  const nodes: ErNode[] = [];
  for (const row of collResult.rows) {
    const name = cellStr(row[nameIdx]);
    if (!name) continue;
    // Sample one document for field names (best-effort)
    let columns: ErColumn[] = [
      { id: colId(dbName, name, "_id"), name: "_id", dataType: "ObjectId", pk: true },
    ];
    try {
      const sample = await executeEngineQuery({
        sessionId,
        text: "{}",
        collection: name,
        database: dbName,
        maxRows: 1,
      });
      if (sample.columns.length > 0) {
        columns = sample.columns.map((c) => ({
          id: colId(dbName, name, c.name),
          name: c.name,
          dataType: c.dataType ?? "mixed",
          pk: c.name === "_id",
        }));
      }
    } catch {
      /* keep default _id */
    }
    nodes.push({
      id: nodeId(dbName, name),
      kind: "collection",
      name,
      columns,
      x: 0,
      y: 0,
    });
  }

  return {
    diagram: {
      family: "nosql",
      nodes: layoutNodes(nodes),
      edges: [],
    },
    title: `Model · ${dbName}`,
    summary: `${nodes.length} collections (inferred fields · no declared FKs)`,
  };
}

type ColMap = {
  table: string;
  column: string;
  type: string;
  nullable: string;
  key: string;
  schema: string | null;
};

function buildFromColumnRows(
  family: DataFamily,
  result: QueryResult,
  map: ColMap,
  defaultSchema: string | null,
): ErDiagramState {
  const tIdx = colIndex(result, map.table);
  const cIdx = colIndex(result, map.column);
  const tyIdx = colIndex(result, map.type);
  const nIdx = colIndex(result, map.nullable);
  const kIdx = colIndex(result, map.key);
  const sIdx = map.schema ? colIndex(result, map.schema) : -1;

  type Acc = { schema: string; name: string; columns: ErColumn[] };
  const tables = new Map<string, Acc>();

  for (const row of result.rows) {
    const table = cellStr(row[tIdx]);
    const column = cellStr(row[cIdx]);
    if (!table || !column) continue;
    const schema =
      sIdx >= 0 ? cellStr(row[sIdx]) : defaultSchema ?? "";
    const key = `${schema}::${table}`;
    let acc = tables.get(key);
    if (!acc) {
      acc = { schema, name: table, columns: [] };
      tables.set(key, acc);
    }
    const rawType = tyIdx >= 0 ? cellStr(row[tyIdx]) : "unknown";
    const nullableRaw = nIdx >= 0 ? cellStr(row[nIdx]).toUpperCase() : "YES";
    const keyRaw = kIdx >= 0 ? cellStr(row[kIdx]).toUpperCase() : "";
    acc.columns.push({
      id: colId(schema, table, column),
      name: column,
      dataType: rawType || "unknown",
      pk: keyRaw.includes("PRI") || keyRaw === "1",
      nullable: nullableRaw === "YES" || nullableRaw === "1",
    });
  }

  const nodes: ErNode[] = [...tables.values()].map((t) => ({
    id: nodeId(t.schema, t.name),
    kind: family === "nosql" ? "collection" : "table",
    name: t.schema && t.schema !== "public" && t.schema !== "main"
      ? `${t.schema}.${t.name}`
      : t.name,
    columns: t.columns,
    x: 0,
    y: 0,
  }));

  return { family, nodes, edges: [] };
}

type FkMap = {
  fromTable: string;
  fromColumn: string;
  toTable: string;
  toColumn: string;
  fromSchema: string | null;
  toSchema: string | null;
};

function addFkEdges(
  diagram: ErDiagramState,
  result: QueryResult,
  map: FkMap,
  defaultSchema: string | null,
) {
  const ft = colIndex(result, map.fromTable);
  const fc = colIndex(result, map.fromColumn);
  const tt = colIndex(result, map.toTable);
  const tc = colIndex(result, map.toColumn);
  const fs = map.fromSchema ? colIndex(result, map.fromSchema) : -1;
  const ts = map.toSchema ? colIndex(result, map.toSchema) : -1;

  const edges: ErEdge[] = [];
  let i = 0;
  for (const row of result.rows) {
    const fromTable = cellStr(row[ft]);
    const fromColumn = cellStr(row[fc]);
    const toTable = cellStr(row[tt]);
    const toColumn = cellStr(row[tc]);
    if (!fromTable || !fromColumn || !toTable || !toColumn) continue;
    const fromSchema =
      fs >= 0 ? cellStr(row[fs]) : defaultSchema ?? "";
    const toSchema = ts >= 0 ? cellStr(row[ts]) : defaultSchema ?? "";
    const fromId = nodeId(fromSchema, fromTable);
    const toId = nodeId(toSchema, toTable);
    // Skip if nodes missing (shouldn't happen)
    if (!diagram.nodes.some((n) => n.id === fromId)) continue;
    if (!diagram.nodes.some((n) => n.id === toId)) continue;
    edges.push({
      id: `e_fk_${i++}_${fromTable}_${fromColumn}`,
      from: fromId,
      to: toId,
      fromColumnId: colId(fromSchema, fromTable, fromColumn),
      toColumnId: colId(toSchema, toTable, toColumn),
      kind: "fk",
      label: `${fromTable}.${fromColumn} → ${toTable}.${toColumn}`,
      waypoints: [],
    });
  }
  diagram.edges = edges;
}
