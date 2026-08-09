/**
 * Build a live ER / model diagram from plugin catalog + relationships.
 * Layout stays in the FE; SQL / FK discovery lives in engine plugins.
 */
import {
  listEngineCatalogColumns,
  listEngineRelationships,
  type CatalogColumnRow,
  type CatalogRelationship,
} from "./engines";
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
  summary: string;
};

export async function loadLiveErDiagram(
  sessionId: string,
  engine: string,
  database: string | null | undefined,
): Promise<LiveErLoadResult> {
  const family = familyForEngine(engine);
  const db = (database ?? "").trim() || null;
  const catalogCols = await listEngineCatalogColumns({
    sessionId,
    database: db,
  });
  const rels = await listEngineRelationships({
    sessionId,
    database: db,
  });
  const diagram = buildFromCatalogColumns(family, catalogCols, db);
  addRelationshipEdges(diagram, rels);

  const title =
    family === "nosql"
      ? `Model · ${db || "database"}`
      : db
        ? `ER · ${db}`
        : "ER diagram";
  const relLabel =
    family === "nosql"
      ? "inferred fields · no declared FKs"
      : `${diagram.edges.length} relationships`;
  return {
    diagram: { ...diagram, nodes: layoutNodes(diagram.nodes) },
    title,
    summary: `${diagram.nodes.length} ${family === "nosql" ? "collections" : "tables"} · ${relLabel}`,
  };
}

function buildFromCatalogColumns(
  family: DataFamily,
  rows: CatalogColumnRow[],
  defaultSchema: string | null,
): ErDiagramState {
  type Acc = { schema: string; name: string; columns: ErColumn[] };
  const tables = new Map<string, Acc>();

  for (const row of rows) {
    const table = row.table;
    const column = row.column;
    if (!table || !column) continue;
    const schema = row.schema?.trim() || defaultSchema || "";
    const key = `${schema}::${table}`;
    let acc = tables.get(key);
    if (!acc) {
      acc = { schema, name: table, columns: [] };
      tables.set(key, acc);
    }
    acc.columns.push({
      id: colId(schema, table, column),
      name: column,
      dataType: row.dataType || "unknown",
      pk: !!row.pk,
      nullable: row.nullable,
    });
  }

  const nodes: ErNode[] = [...tables.values()].map((t) => ({
    id: nodeId(t.schema, t.name),
    kind: family === "nosql" ? "collection" : "table",
    name:
      t.schema && t.schema !== "public" && t.schema !== "main"
        ? `${t.schema}.${t.name}`
        : t.name,
    columns: t.columns,
    x: 0,
    y: 0,
  }));

  return { family, nodes, edges: [] };
}

function addRelationshipEdges(
  diagram: ErDiagramState,
  rels: CatalogRelationship[],
) {
  const nodeIds = new Set(diagram.nodes.map((n) => n.id));
  const edges: ErEdge[] = [];
  let i = 0;
  for (const rel of rels) {
    const fromSchema = rel.fromSchema?.trim() || "";
    const toSchema = rel.toSchema?.trim() || "";
    const fromId = nodeId(fromSchema, rel.fromTable);
    const toId = nodeId(toSchema, rel.toTable);
    if (!nodeIds.has(fromId) || !nodeIds.has(toId)) continue;
    edges.push({
      id: `e_${i++}_${fromId}_${toId}`,
      from: fromId,
      to: toId,
      fromColumnId: colId(fromSchema, rel.fromTable, rel.fromColumn),
      toColumnId: colId(toSchema, rel.toTable, rel.toColumn),
      kind: "fk",
    });
  }
  diagram.edges = edges;
}
